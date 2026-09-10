# 云端发布 Windows 版本

GitHub Actions 使用 Windows 云端机器完成测试、编译、NSIS 打包、更新签名及 Release 发布。本机只需修改代码、版本和更新说明后推送，不需要编译安装包。

## 首次配置

在仓库 Settings → Secrets and variables → Actions 中设置 `TAURI_SIGNING_PRIVATE_KEY`，内容为现有更新私钥文件的完整文本。必须继续使用与应用中公钥匹配的原密钥，否则旧版本无法自动更新。私钥只放在 Actions Secret，不能提交到 Git。

当前私钥没有密码，`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 可以不配置。密钥在构建时临时写入云端 runner 临时目录，任务结束清理。构建脚本沿用 `scripts/build-update-release.ps1` 的安装包类型标记校正和更新清单格式。

## 每次发布

1. 递增应用版本，同步 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`Cargo.lock` 和 `src-tauri/tauri.conf.json`。
2. 在 `.github/release-notes/` 新增对应标签的说明，例如 `v_41.md`，用普通用户能看懂的语言写清功能入口和用法。
3. 提交并推送 main，给该提交创建下一个 `v_数字` 标签并推送标签。
4. 在 GitHub Actions 查看 `Build and publish Windows release`。测试、打包和资产大小/SHA-256 校验全部通过后，自动发布 Release 并更新 Latest；失败时不会发布不完整的版本。

也可以在 Actions 页面手动运行工作流，填写已有标签进行构建。已经公开的 Release 不会被覆盖；如果上传中途失败留下草稿，先检查草稿资产，不能用不同构建产物覆盖已公开版本。

构建固定 Node 24.14.0、Rust 1.94.1，与当前项目环境一致；依赖由锁文件安装，npm 和 Rust 依赖使用云端缓存。工作流不会更新本机环境。`latest.json` 与安装包上传到同一个 Release，保留应用内更新入口。
