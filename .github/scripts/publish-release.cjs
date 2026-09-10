const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

module.exports = async ({ github, context, core }) => {
  const tag = process.env.RELEASE_TAG;
  if (!/^v_\d+$/.test(tag)) throw new Error('Invalid release tag');
  const version = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
  const notes = fs.readFileSync(`.github/release-notes/${tag}.md`, 'utf8').trim();
  const directory = path.join(process.env.RUNNER_TEMP, 'release', tag);
  const installer = `Smart-Spreadsheet_${version}_x64-setup.exe`;
  const names = [installer, `${installer}.sig`, 'latest.json'];
  const files = names.map(name => {
    const data = fs.readFileSync(path.join(directory, name));
    if (!data.length) throw new Error(`Empty release asset: ${name}`);
    return { name, data, digest: `sha256:${crypto.createHash('sha256').update(data).digest('hex')}` };
  });
  const manifest = JSON.parse(files[2].data.toString('utf8').replace(/^\uFEFF/, ''));
  const platform = manifest.platforms?.['windows-x86_64'];
  const expectedUrl = `https://github.com/${context.repo.owner}/${context.repo.repo}/releases/download/${tag}/${installer}`;
  if (manifest.version !== version || manifest.notes !== notes || platform?.url !== expectedUrl ||
      platform.signature !== files[1].data.toString('utf8').trim()) {
    throw new Error('Updater manifest does not match the version, notes, URL or signature');
  }
  let release;
  try {
    release = (await github.rest.repos.getReleaseByTag({ ...context.repo, tag })).data;
  } catch (error) {
    if (error.status !== 404) throw error;
  }
  if (release && !release.draft) throw new Error(`${tag} is already published; never overwrite a public release`);
  if (!release) {
    release = (await github.rest.repos.createRelease({
      ...context.repo, tag_name: tag, name: `${tag} · 智能表格 ${version}`,
      body: notes, draft: true, prerelease: false,
    })).data;
  }
  const existing = await github.paginate(github.rest.repos.listReleaseAssets, { ...context.repo, release_id: release.id });
  for (const file of files) {
    const found = existing.find(asset => asset.name === file.name);
    if (found) {
      if (found.digest !== file.digest) throw new Error(`Draft asset differs: ${file.name}; inspect the draft before retrying`);
      continue;
    }
    const uploaded = (await github.rest.repos.uploadReleaseAsset({
      ...context.repo, release_id: release.id, name: file.name, data: file.data,
      headers: { 'content-type': 'application/octet-stream', 'content-length': file.data.length },
    })).data;
    if (uploaded.size !== file.data.length || uploaded.digest !== file.digest) {
      throw new Error(`Uploaded asset failed size/hash verification: ${file.name}`);
    }
    core.info(`${file.name}: ${file.digest}`);
  }
  await github.rest.repos.updateRelease({
    ...context.repo, release_id: release.id, body: notes, draft: false, make_latest: 'true',
  });
  await core.summary.addHeading(`${tag} published`).addLink('Download release', release.html_url)
    .addTable([['Asset', 'SHA-256'], ...files.map(file => [file.name, file.digest.slice(7)])]).write();
};
