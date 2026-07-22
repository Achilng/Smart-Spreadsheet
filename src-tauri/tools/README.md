# Bundled artist dictionary

The release bundle contains a gzip-compressed Danbooru artist dictionary at
`resources/artist-dictionary.json.gz`. Refresh it in two stages so the shipped
file contains final lookup entries instead of the much larger raw API payload.

On Windows, keep resumable API pages and the raw snapshot under the Agent temp
directory:

```powershell
python src-tauri/tools/update_artist_dictionary_snapshot.py `
  D:\Agent\Agent_temp\artist-dictionary-raw.json.gz `
  --cache D:\Agent\Agent_temp\artist_dictionary_pages

cargo run -p smart-spreadsheet --example build_artist_dictionary_bundle -- `
  D:\Agent\Agent_temp\artist-dictionary-raw.json.gz `
  src-tauri/resources/artist-dictionary.json.gz

cargo run -p smart-spreadsheet --example verify_artist_dictionary_bundle -- `
  src-tauri/resources/artist-dictionary.json.gz
```

The downloader uses Danbooru's before-ID cursor, retries transient failures and
caches every completed page. The Rust compiler applies the same identity merge
logic as the application, then writes a streaming JSON-lines gzip file. The
first line is the snapshot header; each following line is one final lookup
entry.
