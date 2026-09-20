# 代码组织与修改入口

本项目是本地资料库应用。一次操作通常经过：页面 → 功能操作或状态模块 → API → Tauri 命令 → 运行时 → 数据库 / 文件存储。窗口各自持有前端状态，通过明确的事件通知其它窗口刷新。

## 前端

| 目录 / 模块 | 负责什么 | 修改约定 |
| --- | --- | --- |
| `src/lib/views/` | 页面布局、组件组合、输入事件 | 复杂业务流程交给对应功能模块；样式保留在组件中 |
| `src/lib/features/` | 规则、快速整理、素材、文档、图片导出、资料库操作 | 页面控制器在组件初始化时创建，状态与生命周期属于该页面实例 |
| `stores/library-state.svelte.ts` | 当前资料库快照与查询版本 | 不放窗口布局、任务进度或文件操作 |
| `stores/workspace-state.svelte.ts` | 视图、面板开关和尺寸 | 只保存当前窗口的展示偏好 |
| `stores/task-state.svelte.ts`、`tasks.ts` | 进度、忙碌状态、操作错误收尾 | 这是当前窗口状态，不是后端全局互斥锁 |
| `stores/notices.svelte.ts` | 通知队列与消失计时 | 所有通知从此入口创建 / 清除 |
| 其它 `stores/` | 行缓存、选区、Tag、分组、历史、导航 | 保留现有业务范围；通过事件或登记回调解开互相引用 |
| `src/lib/api/` | 请求 / 响应类型和 IPC 调用 | 不操作页面状态，不显示通知 |
| `src/lib/windows/` | 窗口事件与导航 | 协议与调用方业务操作分开 |
| `src/lib/ui/`、`images/`、`utils/` | 公共界面、图片加载、纯计算 | 公共工具不反向导入具体页面 |

`controller.svelte.ts` 中的 `$state`、`$derived` 和生命周期函数需要在组件创建控制器时注册。不要将这些工厂的返回值提升为模块单例；需要跨页面共享的状态应有明确的 store。

新增常见逻辑前先查以下入口：

- Tag / 列表文本：`utils/list-text.ts`；规则动作默认值：`features/automation/rule-defaults.ts`。
- 查询公共筛选快照：`utils/library-query.ts`。分页、选择范围、分组排除等仍由调用方显式补充。
- 画廊和素材网格：`images/gallery-layout.ts`；画廊 / 表格恢复滚动：`stores/viewport-scroll.svelte.ts`；位置记忆：`stores/view-state.ts`。
- 工具箱定位主窗口图片：`windows/toolbox.ts`；资料库变更通知：`windows/library-events.ts`。
- 详情文本编辑：`features/library/field-editor.svelte.ts`；错误和数量格式：`utils/format.ts`。
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
