# 代码组织与修改入口

本项目是本地资料库应用。一次操作通常经过：页面 → 功能操作或状态模块 → API → Tauri 命令 → 运行时 → 数据库 / 文件存储。窗口各自持有前端状态，通过明确的事件通知其它窗口刷新。

## 前端

前端是 React（入口 `src/main.ts` → `src/react/main.tsx`）。主窗口、工具箱、对比窗口和整理助手共用同一个页面入口，按地址参数 `?window=` 选择窗口；开发时加 `?mock` 使用 `src/lib/dev/` 的模拟后端。

| 目录 / 模块 | 负责什么 | 修改约定 |
| --- | --- | --- |
| `src/react/views/` | 各窗口的页面、弹窗和工具箱工具 | 复杂业务流程交给 `state/`；页面专属样式放在同目录 |
| `src/react/state/` | Zustand 状态与业务操作：资料库快照、工作区、选区、任务、通知、历史等 | 跨页面共享的状态都在这里，按功能一文件 |
| `src/react/ui/` | 公共控件与交互钩子；基础控件集中在 `controls.tsx`（基于 Radix） | 新页面优先复用这里的控件，不直接写浏览器原生下拉框等 |
| `src/react/styles/` | 色板、基础样式、共用控件和页面框架样式 | 颜色和尺寸使用 `tokens.css` 变量 |
| `src/lib/api/` | 请求 / 响应类型和 IPC 调用 | 不操作页面状态，不显示通知 |
| `src/lib/windows/` | 窗口事件与导航 | 协议与调用方业务操作分开 |
| `src/lib/images/`、`utils/`、`stores/`、`features/` | 图片加载、布局计算、纯函数工具及少量与界面框架无关的状态 | 不依赖 React，单元测试直接引用这里 |

新增常见逻辑前先查以下入口：

- Tag / 列表文本：`utils/list-text.ts`；规则动作默认值：`features/automation/rule-defaults.ts`。
- 查询公共筛选快照：`utils/library-query.ts`。分页、选择范围、分组排除等仍由调用方显式补充。
- 画廊和素材网格：`images/gallery-layout.ts`；位置记忆：`stores/view-state.ts`。
- 工具箱定位主窗口图片：`windows/toolbox.ts`；资料库变更通知：`windows/library-events.ts`。
- 详情文本编辑：`src/react/ui/FieldEditor.tsx`；错误和数量格式：`utils/format.ts`。
- 视图标识、顺序和文案：`utils/view-modes.ts`。

## 后端

| 目录 / 模块 | 负责什么 |
| --- | --- |
| `src-tauri/src/app/commands/` | 按功能接收 IPC 参数、安排阻塞任务、发送进度事件、转换返回值 |
| `app/runtime/mod.rs` | 资料库定位、生命周期、锁和连接访问 |
| `app/runtime/` 其它模块 | 对应功能的运行时操作；不集中堆入入口文件 |
| `automation/` | 规则模型、校验、匹配、内存动作和文本处理 |
| `db/automation_rules/` | 规则存取、事务执行、可移植格式与数据库依赖映射 |
| `db/quick_edit/` | Tag、分组、画师前缀各自的预览 / 执行 / 撤回，以及共用条件匹配 |
| `storage/import_images/` | 追加导入、更新导入及共用来源准备、文件、元数据、进度处理 |
| `storage/rule_files.rs` | 规则文件读写、大小限制、摘要和安全替换 |
| `pipeline/prompt_text.rs` | 提示词权重与画师文本基础处理；业务专用别名留在调用方 |
| `storage/image_paths.rs` | 内容哈希和感知哈希共用的图片候选路径；完整原件解析保留独立约束 |

命令注册集中在 `src-tauri/src/lib.rs`。新增或移动命令时保持前端调用名、参数名及事件协议一致。数据库方法的事务边界留在数据库模块内，不能为了复用将写入拆散。原有规则错误类型仍通过 `db` 重导出以兼容调用方，其中数据库错误包装是保留的类型依赖。

## 验证与回档

常规检查沿用现有命令，不新增测试框架：

```powershell
npm.cmd run test:unit
npm.cmd run check
npm.cmd run build
cargo test --workspace --lib --locked --offline
```

结构重构的范围、验证记录及 Git 回档方式见 [REFACTORING.md](REFACTORING.md)。当前待办见 [../PROGRESS.md](../PROGRESS.md)。
