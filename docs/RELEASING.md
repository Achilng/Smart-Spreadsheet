# 发布 Windows 版本

GitHub Actions 使用 Windows 云端机器完成测试、编译、NSIS 打包、更新签名及 Release 发布。本机只需修改代码、版本和更新说明后推送，不需要编译安装包。

## 用户要求本机编译时

1. 同步下文所列的五处版本号，并写好对应的 `.github/release-notes/v_数字.md`。在本机运行 `npm.cmd run test:unit` 和 `cargo test --workspace --lib --locked`。
2. 把进程的 `TEMP` / `TMP`、npm 缓存和 Node 编译缓存指向 `D:/Agent/Agent_temp`。沿用已安装的 Node、Rust、NSIS 和现有更新签名密钥，不升级环境。
3. 设置与云端工作流一致的 `VITE_STYLE_EXTRACTOR_URL` 和 `VITE_STYLE_REVIEW_URL`，然后运行 `scripts/build-update-release.ps1 -ReleaseTag v_数字 -NotesPath .github/release-notes/v_数字.md`。脚本在本机编译、校正安装包类型标记、打包并签名，产物位于 `target/release/publish/v_数字/`。
4. 发布提交消息包含 `[skip ci]`，防止推送发布标签时重复触发云端构建。推送代码及标签后，核查该提交没有正在运行的云端发布任务。
5. 创建 Release 草稿，上传安装器、`.sig` 和 `latest.json`，核对更新清单的版本、下载地址、签名，以及三个资产的大小和 SHA-256，再公开并设为最新版本。可复用 `.github/scripts/publish-release.cjs` 的校验逻辑；不覆盖已公开版本。发布后重新获取公开更新清单并下载安装器核对哈希。
6. 需要同时更新本机时，先备份 `D:/应用/智能表格/smart-spreadsheet.exe`，再替换为本次构建的程序；替换前确认该安装路径的进程已退出。直接使用未打包 exe 时需按构建脚本同样方式写入 NSIS 类型标记，校验安装文件、旧版备份和安装包中的程序，资料库继续使用原目录。

GitHub CLI 的使用仍须遵循本机授权约定；发布请求本身不取消对 `gh` 命令的单独限制。

## 首次配置

在仓库 Settings → Secrets and variables → Actions 中设置 `TAURI_SIGNING_PRIVATE_KEY`，内容为现有更新私钥文件的完整文本。必须继续使用与应用中公钥匹配的原密钥，否则旧版本无法自动更新。私钥只放在 Actions Secret，不能提交到 Git。

当前私钥没有密码，`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 可以不配置。密钥在构建时临时写入云端 runner 临时目录，任务结束清理。构建脚本沿用 `scripts/build-update-release.ps1` 的安装包类型标记校正和更新清单格式。

## 每次发布

1. 递增应用版本，同步 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`Cargo.lock` 和 `src-tauri/tauri.conf.json`。
2. 在 `.github/release-notes/` 新增对应标签的说明，例如 `v_41.md`，用普通用户能看懂的语言写清功能入口和用法。
3. 提交并推送 main，给该提交创建下一个 `v_数字` 标签并推送标签。
4. 在 GitHub Actions 查看 `Build and publish Windows release`。测试、打包和资产大小/SHA-256 校验全部通过后，自动发布 Release 并更新 Latest；失败时不会发布不完整的版本。

也可以在 Actions 页面手动运行工作流，填写已有标签进行构建。已经公开的 Release 不会被覆盖；如果上传中途失败留下草稿，先检查草稿资产，不能用不同构建产物覆盖已公开版本。

构建固定 Node 24.14.0、Rust 1.94.1，与当前项目环境一致；依赖由锁文件安装，npm 和 Rust 依赖使用云端缓存，Rust 缓存在失败时也尝试保存，方便修复后重试。工作流不会更新本机环境。`latest.json` 与安装包上传到同一个 Release，保留应用内更新入口。

首次云端发布 `v_40` 已完成：[构建记录](https://github.com/Achilng/Smart-Spreadsheet/actions/runs/34487023700)、[下载页面](https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_40)。
