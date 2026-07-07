# 开发进度

最后更新：2026-07-07

## 当前阶段

v_18 已发布。移除 xlsx 导入、拖拽严格原图、vibe 标识。

### M49 — 移除 xlsx 导入 + 拖拽严格原图 + vibe 标识（2026-07-07）

- **移除 xlsx 导入功能**：删除 `storage::import` 模块和 `excel::ooxml` 模块，仅保留 xlsx 导出和 v1→v2 迁移遗留的嵌入图提取。xlsx 嵌入图为无元数据缩略图，已不再作为导入来源。
- **拖拽严格使用原图**：新增 `resolve_original_source` 函数，拖拽时优先使用外部原图路径（文件夹来源），允许压缩包受管副本（元数据完整），但拒绝 xlsx 嵌入副本（无元数据）。原图不可用时弹出 toast 错误提示，不再静默降级到无元数据副本。
- **RowImageLocator 扩展**：新增 `source_type` 字段（JOIN `import_batches` 表），区分 Folder/Archive/Xlsx 来源以决定受管副本是否可信。
- **Vibe 标识**：DetailPanel 图片预览区新增紫色 vibe 徽章，解析 PNG Comment JSON 中的 `reference_image_multiple` 字段并显示引用数量（如 `VIBE ×3`），提示拖到 NovelAI 可一并导入。
- **新增 `get_row_vibe_status` 命令**：后端读取 PNG tEXt Comment chunk、解析 JSON、统计 vibe 引用数。
- **测试全部迁移**：原 xlsx 导入测试改为文件夹导入方式，新增 `test_fixtures` 模块生成带 tEXt 元数据的真实 16×16 PNG。157 项测试通过。
- **Release v_18** 已发布至 GitHub。

### M48 — 前端重构（2026-07-03）

- **阶段 0 目录重组**：`src/lib/` 重组为 api/（11 文件）、images/、stores/（16 模块）、ui/（6 组件）、views/（9 组 36 组件）。SectionMembers 类型独立解耦。api.ts 102 个导出逐一核对无丢失。
- **阶段 1 设计令牌**：:root 完整重写——色彩/阴影/圆角/字号/间距/z-index 全部令牌化。25 处 z-index、~150 处 font-size、~25 处 border-radius 硬编码消除。新增全局 .chip/.chip-more/.empty-state 工具类，删除 GalleryCard/TableRow/DetailPanel 等组件内重复定义。
- **阶段 2 Modal 与菜单基础设施**：新增 Modal.svelte（ESC/遮罩点击/aria/焦点管理）和 ContextMenuShell.svelte（外部点击/ESC/统一样式）。7 个弹窗/菜单迁移上去，消除 3 种不一致的关闭行为，a11y warnings 从 10 降到 5。
- **阶段 3 布局重排**：新增左侧 NavRail（52px 图标导航栏 + tooltip + 拖拽区），承载 6 个视图切换；TopBar 瘦身为 40px（搜索+尺寸+导入导出+窗口控制）；Workspace 改为水平布局。
- **阶段 4 逻辑收编**：createSectionCache 工厂消除 group-browse-store/duplicate-browse-store 的成员加载重复；GalleryCard/GroupSectionCard/SearchResultsView 统一走 Thumbnail.svelte + 全局缩略图单例。
- **阶段 5 逐视图精修**：DetailPanel 正/负向提示词 8 个 state + 6 个函数合并为 createPromptEditor 工厂 × 2 实例 + 模板 each 循环，净减 36 行。
- **总体**：前端从 57 文件 11663 行变为 72 文件 11489 行（净减 ~170 行），但实际删除重复代码远多于此（被新增的基础设施和拆分后的文件头开销抵消）。svelte-check 0 错误、Vite 生产构建通过。

### M48 — 前端重构·阶段 0 目录重组（2026-07-03）

- `src/lib/` 重组为 `api/`（api.ts 按 11 个领域文件拆分 + barrel，102 个导出与 62 个 invoke 调用逐一核对无丢失）、`images/`、`stores/`（16 个状态模块）、`ui/`（4 个通用组件）、`views/`（shell/gallery/table/groups/duplicates/albums/prompt-docs/search/tools 九组 36 个组件）。
- 全部 `git mv` 保留历史；除 import 路径外唯一实质改动是 `SectionMembers` 抽到 `stores/section-types.ts`，消除 duplicate-browse-store → group-browse-store 的类型耦合。
- 验收：`svelte-check` 0 错误（10 个 a11y 警告为既有基线）、Vite 生产构建通过。

### M47 — 位置记忆失效修复（2026-07-02 ~ 2026-07-03）

- 与用户重新对齐需求：期望行为不是“切回来后猜测恢复滚动条”，而是类似 Chrome 标签页，切到其他视图再回来时画廊页面仍停在原处，DOM、滚动容器、已加载分页和当前内容不应被销毁或改写。落实为 keep-alive 方案：Workspace 对已访问的主视图保活（隐藏而非卸载），画廊/表格/分组/重复/画册各自接收 `active` 标记；隐藏视图不执行补页、聚合同步、缩略图 retain、大图预览和全局快捷键副作用，避免后台空转。
- 修复更深层根因：顶栏切换到「分组」时不再调用 `setGroupView()`，因此不会把共享 `rowStore` 切换成“分组代表行”查询语义，也不会让画廊背后的结果集变短、spacer 高度缩水、滚动位置被浏览器钳回顶部。分组浏览视图继续使用独立的分组查询与成员缓存。
- 用户再次反馈：画廊滚到约 500 行，切到分组视图再切回画廊仍可能回到第 1 行。复查发现画廊/表格仍使用一次性 `scrollTop = saved`，没有接入 M47 新增的按帧重试恢复；若刚切回时视口尺寸或虚拟 spacer 高度尚未就绪，浏览器会把目标位置钳回 0，且本地虚拟滚动状态不同步。修复：画廊/表格统一改用 `restoreScrollPosition()`，等待 viewport 尺寸和 spacer 高度有效后再恢复，并通过回调同步组件内 `scrollTop`；组件销毁前额外保存一次真实滚动位置，避免切换瞬间漏记，同时用滚动记忆版本号防止筛选/数据重置清空后被旧位置反写。
- 用户实测（`npm run tauri dev`）反馈"根本没记住"。用 Playwright + 假 Tauri IPC（`window.__TAURI_INTERNALS__` 注入，真实前端代码在 Edge 里跑）复现，7 个往返场景中 2 个失败，均为**恢复时机早于内容高度就绪，scrollTop 被浏览器钳制**：
  1. 画廊/表格 ↔ 分组视图往返：`setGroupView` 触发 rowStore 换代刷新，group_view 查询把每组折叠为一行代表，totalCount 大幅缩水；切回时恢复逻辑在新结果落地前执行，虚拟滚动 spacer 高度不足（复现：20000px 被钳到 2088px）。修复：恢复条件增加 `!rowStore.refreshing`，等在途刷新落地、totalCount 恢复正确语义后再恢复。
  2. 分组视图列表：成员网格 `content-visibility: auto` 离屏只有 500px 预估高度 + 成员异步加载，恢复瞬间总高度不足（复现：800px 被钳到 142px）。修复：新增 `restoreScrollPosition()` 助手（view-state.ts），按帧重试直到 scrollTop 生效，用户主动滚动或元素卸载即放弃；分组/重复/画册三个分区视图统一改用。
- 修复后 7 场景全部 PASS（含画廊经分组往返、分组展开状态保留）；`svelte-check` 0 错误。复现脚本在 `D:\Agent\Agent_temp\ss-scroll-repro`（临时目录，含 mock-tauri.js 与 repro.mjs）。
- **保活方案的最终根因修复（2026-07-03）**：用真实前端 + 假 Tauri IPC 复现出保活方案残留的失效链条——面板 `display:none` 后 Svelte 尺寸绑定（ResizeObserver）报 0，画廊列数/卡宽退化把 spacer 改写成 `totalCount×47px` 的错误高度；切回第一帧浏览器按这个退化高度钳制 scrollTop（实测 280000px 被钳到 140168px），钳制触发的 scroll 事件又把错误值写进位置记忆，且 `restored` 标志一次性生效导致永无兜底。三个实证的浏览器行为：① Chromium 会跨 `display:none` 保留 scrollTop；② 但按重新显示第一帧的内容高度钳制，之后高度恢复也不回弹、且应用侧只收到一次普通 scroll 事件；③ 对 `display:none` 元素设置 scrollTop 被静默忽略（导致隐藏期间的"回顶"无效、虚拟单元格与真实偏移脱同步）。修复（GalleryView/TableView/GroupBrowseView/DuplicateBrowseView/AlbumBrowseView/view-state.ts）：尺寸绑定改经冻结层（报 0 时保留最后有效值，隐藏期间布局不变，顺带覆盖窗口最小化场景）；`restored` 在失活时复位，每次切回重跑按帧恢复作为钳制兜底；`restoreScrollPosition` 新增 onSettled 回调，恢复期间和非激活状态的 scroll 读数不写入位置记忆；resetToken 回顶改为"隐藏时挂起、激活后执行"。回归：Playwright 驱动真实前端 7 场景（浅/深滚动往返、深位置二次往返验记忆未污染、分组视图内搜索后切回回顶、激活态清搜索回顶、表格往返、分组列表自身位置）在 1400/1000px 两种宽度全部 PASS；`svelte-check` 0 错误、Vite 生产构建通过。

### M46 — 性能改造全量验证（2026-07-02）

- `cargo test`：165 单测 + 2 集成全部通过；`cargo clippy --all-targets --all-features -- -D warnings` 零告警。
- 万行基准复跑（release）：无筛选首查 2.7ms / 缓存翻页 3.3ms；双 Tag AND 5.7ms / 1.8ms；文本搜索 15.5ms / 1.0ms；画师串去重 8.4ms——与 M42 记录同量级。
- `npm.cmd run build`：`svelte-check` 0 错误（保留既有 10 个 a11y 警告）、Vite 生产构建通过。
- `tauri build`：Windows x64 NSIS 打包成功（`智能表格_0.9.2_x64-setup.exe`，4,385,312 字节，本地验证构建，未发布）。

### M45 — 浅色精致化换肤（2026-07-02）

- **设计令牌微调**：背景加深一档（#eef1f5）让白色卡片更立体，文字三级灰度微调；新增 `--shadow-hover`（卡片悬浮投影）与 `--focus-ring`（统一焦点环）。
- **全局细滚动条**：替换 Windows WebView 默认粗灰滚动条为 10px 圆角细条，悬停加深；`::selection` 用主题色。
- **统一焦点体验**：全局 `:focus-visible` 键盘焦点环（鼠标点击不出现）；顶栏搜索、Tag 创建、提示词文档搜索、各对话框输入框聚焦统一为「accent 边框 + 柔和光环」，清理了两处遗留的未定义 `--primary` 变量。
- **画廊卡片**：悬停微抬升（-1px + 柔和投影）；缩略图加载占位从"…"文字改为微光 shimmer（画廊卡、表格行、分组/重复成员卡、画册胶片条共用全局 `.shimmer`）。
- **分区标题吸顶**：分组/重复视图的分区标题滚动时 sticky 吸顶，长列表中始终能看清当前分区。
- 顶栏视图切换按钮：选中态加粗、未选中悬停提亮，过渡动画统一 0.12s。
- 验证：`svelte-check` 0 错误（保留既有 10 个 a11y 警告）、Vite 生产构建通过。

### M44 — 视图位置记忆 + 重复/画册聚合缓存（2026-07-02）

- **状态提升为模块级 store**：分组浏览（`group-browse-store.svelte.ts`）、重复视图（`duplicate-browse-store.svelte.ts`）、画册（`album-store.svelte.ts`）的聚合结果、展开状态、已加载成员和排序偏好从组件内提到模块级，切走再切回不再重新聚合、重新展开——秒开。
- **缓存按签名失效**：分组成员缓存看 `dataVersion + membershipVersion`（新增，分配/移出/删除分组时 +1，覆盖分组对话框、建议分组、管理分组全部入口）；未分组区叠加筛选/搜索参数；重复视图聚合看 `dataVersion + 筛选 + 聚合依据`（顺带修复旧版不看 `dataVersion` 的隐性脏数据 bug），别名改名只静默刷新列表、保留展开状态；画册列表看 `dataVersion + aliasVersion`。失效后已展开的分组自动重新拉取成员。
- **滚动位置记忆**：分组/重复/画册列表容器接入 `view-state.ts`（键 `groups`/`duplicates`/`albums`），与画廊/表格（M43）行为一致：切视图保留，筛选/数据变化清空回顶。
- **画册阅读进度**：每本画册记住上次看到第几张，重新打开时自动补加载到目标页并钳位（数据变化可能越界）；胶片条自动把当前图滚动到可见位置。
- **提示词文档**：记住上次打开的文档，切回时恢复选中（文档已删则回退第一篇）。
- 聚合列表重载采用与行数据一致的 stale-while-revalidate：有旧内容时保持渲染后台替换，不再闪"正在加载…"。
- 验证：`svelte-check` 0 错误（保留既有 10 个 a11y 警告）、Vite 生产构建通过。

### M43 — 前端去白屏 + 缩略图调优（2026-07-02）

- **stale-while-revalidate**：`resetRows` 重构为三种语义——筛选/搜索变化保留旧内容直到新结果首页到达后原子替换并回顶（`refreshing` 期间主区顶部显示 2px 流动进度条，不再白屏闪"正在加载…"）；批量操作（打标/分组/提示词编辑）默认就地刷新且**保留滚动位置**；导入/删除/换库仍走全量重载（缩略图缓存同时清空，行 ID 可能复用）。
- 画廊/表格的回顶时机从 `initialLoading` 改挂 `resetToken`（结果集语义变化时才 +1）；两视图挂载时从 `view-state.ts` 恢复上次滚动位置（位置记忆的第一部分，其余视图在 M44）。
- 缩略图加载并发 4 → 8，内存缓存 120 → 480 张对象 URL，来回滚动/切视图不再反复重拉。
- `画廊 ↔ 分组` 切换（`setGroupView`）标记为"非筛选变化"，不清各视图滚动位置。
- 验证：`svelte-check` 0 错误（保留既有 10 个 a11y 警告）、Vite 生产构建通过。

### M42 — 后端查询层重构（2026-07-02）

- **常驻数据库连接**：`AppRuntime` 不再每个命令都 `open_database()` 新建连接，改为在 `RuntimeState` 持有懒打开的常驻 `Database`；重置/迁移数据目录前先关闭连接释放 Windows 文件句柄。
- **筛选结果缓存**：`query_rows` 把筛选结果物化进跨调用复用的 temp 表并缓存 total_count；筛选参数不变时翻页只做 `LIMIT/OFFSET`（旧实现每翻一页都全量重筛一遍）。所有数据变更统一走 `with_database_mut` / `invalidate_query_cache` 使缓存失效；分组/画册成员查询改用独立 scratch 表避免破坏缓存。
- **集合式 Tag 谓词**：AND/OR 筛选从"对每一行执行相关子查询"改为单次 `IN (... GROUP BY ... HAVING ...)`，SQLite 只物化一次命中集合；`count_selected_rows`、批量打标、去重聚合等共用该谓词的路径一并受益。
- **搜索维持 INSTR 语义**：基准显示缓存后无需引入 FTS5（子串语义还会变），万行库冷查 11ms、缓存翻页 0.7ms。
- 万行基准（release，10000 行 × 约 1.5KB 提示词 × 1.5 万 Tag 关联）：无筛选首查 1.8ms / 缓存翻页 2.1ms；双 Tag AND 首查 4.3ms / 翻页 1.4ms；文本搜索首查 11.2ms / 翻页 0.7ms；画师串去重首查 5.3ms。基准测试保留为 `bench_ten_thousand_row_filters`（`--ignored` 手动运行）。
- 新增回归测试：缓存跨页复用与 bump 失效、scratch 查询不污染缓存、runtime 层打标/删行后查询立即可见。`cargo test` 165 单测 + 2 集成全部通过。

### M41 — 提示词文档中心（2026-07-01）

- 新增独立主视图「提示词」，进入后隐藏 Tag 侧栏、详情面板、选择栏和图片资料库搜索框，显示文档列表 + TipTap 富文本编辑器。
- 文档保存到当前受管数据目录 `prompt-docs/doc-*/`，每个文档有独立 `meta.json`、`content.json` 和 `images/`；迁移数据目录时一起迁移，重置表格不删除提示词文档。
- 支持创建多个文档、标题/正文纯文本搜索、标题和正文 800ms 防抖自动保存、删除二次确认、复制全文纯文本。
- 编辑器支持正文、标题、加粗、斜体、列表，以及正文中插入图片；图片支持文件选择、Tauri 路径拖拽、剪贴板/浏览器文件粘贴，保存时使用相对 `images/xxx`，显示时映射为 Tauri 本地资源 URL。
- 后端新增提示词文档存储模块与 7 个 Tauri 命令；新增 Rust 测试覆盖 CRUD、路径穿越拒绝、图片导入、迁移保留和重置保留。
- 顺手修复当前 Clippy 对 `prompt_edit.rs` 的 `collapsible_if` 风格门禁，不改变画师前缀修正逻辑。
- 全量验证：`npm.cmd run build` 通过（`svelte-check` 0 错误，保留既有 10 个 a11y 警告；Vite 仅提示 TipTap 后 chunk 超过 500KB）、`cargo test` 161 项单测 + 2 项集成测试通过、`cargo clippy --all-targets --all-features -- -D warnings` 通过。

### v0.9.2 — 画师前缀修正发布（2026-06-27）

- 修复「编辑提示词 → 修正画师前缀」语义；`svelte-check` 0 错误、Vite 生产构建通过、Tauri Windows x64 NSIS 打包成功。安装器 `智能表格_0.9.2_x64-setup.exe` 大小 4,241,723 字节，SHA-256 `9E9617C8FE1C31841543A12165C0FCC8909671237FE4CB2C91F538503AD76351`。GitHub Release `v_16` 已发布。

### M40 — 画师前缀修正工具（2026-06-27）

- 修复「编辑提示词 → 修正画师前缀」语义：不再把 `artist:画师名, ` 无条件插到所有选中行开头，而是让用户输入不带 `artist:` 的画师名，只修正选中图片中完全匹配且尚未带前缀的画师 tag。
- 支持常见 NovelAI 权重写法：普通 `parsley_f`、括号权重 `(parsley_f:1.2)`、花/方括号权重 `{parsley_f}` / `[parsley_f]`、以及 `0.7::parsley_f` / `0.5::parsley_f::`，修正后保留原权重结构。
- 前端文案改为「修正画师前缀」，输入框和说明明确提示「画师名不带 artist:」，并说明只处理选中图片中的精确匹配 tag。
- 涉及文件：`prompt_edit.rs`、`api.ts`、`PromptEditDialog.svelte`、`PROGRESS.md`。

### M39 — 分组视图性能优化与排序（2026-06-20）

- **性能优化**：分组视图展开大分组（200 张）时不再瞬间渲染所有卡片。改用 IntersectionObserver 渐进式渲染（每批 40 张，滚动到底部自动加载下一批）；折叠时重置渲染计数。`memberCache` 改为 Svelte 5 直接属性赋值（不再整体 spread），减少不必要的全量 reactivity 触发。`.member-grid` 添加 `content-visibility: auto` + `contain-intrinsic-height`，浏览器自动跳过离屏网格的布局与绘制。
- **按数量排序**：分组视图顶部新增 sticky 工具栏，勾选「按数量排序」后分组按 `memberCount` 降序排列（图片多的排在上面），默认仍按名称排序。
- 涉及文件：`GroupBrowseView.svelte`、`PROGRESS.md`。

### v0.9.1 — 顶栏布局修复（2026-06-19）

- 跳动根因：`SizeSlider`（卡片大小/行高滑块）仅在画廊/表格视图显示，其它视图被移除，导致 `flex:1` 工作簿信息区变宽、把视图切换按钮整块右推。修复：滑块在非画廊/表格视图下隐藏但保留占位（`visibility:hidden`），顶栏不再重排。
- 退回顶栏原右挤布局（一度尝试的居中三段式会让固定宽搜索框/菜单在窗口不够宽时溢出、压住视图按钮，已撤销）。
- 版本提升至 0.9.1；`svelte-check` 0 错误、生产构建通过；Windows x64 NSIS 打包成功。安装器 `智能表格_0.9.1_x64-setup.exe` 大小 4,233,151 字节，SHA-256 `BFC353C68503710ECDBED672EBAD89B69698D89941EB539739F27C60C80EA206`。GitHub Release [`v_14`](https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_14) 已发布。

### M34–M38 — 画师串工具组（2026-06-18）

- **M34 后端基础**：`db/query.rs` 新增 `list_distinct_artists`（全库画师串按换行拆分、trim、去重、排序）与 `row_ids_with_artists`（全库取画师串完全相同的行 ID，忽略 Tag 筛选）；`list_dedupe_clusters` 增加内部 `min_members` 参数（现有「重复」命令外部签名不变，传 2）。自定义画师名单复用 `settings` 表（key `custom-artists`），不提升 schema 版本。新增命令链路 `list_artist_albums` / `list_distinct_artists` / `row_ids_with_artists` / `get_custom_artists` / `set_custom_artists`（runtime + commands + lib.rs 注册 + api.ts）。Rust 测试 155 项通过（新增 3 项）、Clippy 无警告。
- **M35 随机画师串生成器**：工具菜单新增「随机画师串」浮层 `ArtistGeneratorView`。画师池 = 库内画师 + 自定义名单（勾选启用），「只用干净 artist: 片段」开关过滤带 `::` 权重片段；设数量 N，前端 Fisher-Yates 无放回随机抽取拼成 `, ` 连接串，一键复制（`navigator.clipboard`）；自定义名单防抖自动保存。
- **M36 一键选中相同画师串**：缩略图/表格行右键菜单新增「选中相同画师串」（画师串为空时置灰），调用 `row_ids_with_artists` 后经 `setExplicitSelection` 设为当前选择，底部选择栏随即可批量打 Tag / 导出 / 删除。
- **M37 画册集视图**：顶栏新增「画册」视图，复用画师串聚合（`min_members=1`，每个画师串成一册含单张）。`AlbumBrowseView` 列出画册（`AlbumCard` 用 IntersectionObserver 懒加载首张成员作封面），点进 `AlbumReader` 阅读式翻图：大图（`get_row_preview`）+ 上/下一张 + ←/→ 键 + 底部缩略图条，当前图同步右侧详情面板。成员加载复用现有 `get_dedupe_cluster_members`。
- **M38 验收发布**：版本提升至 0.9.0；Rust 155 项测试 + Clippy + `svelte-check` 0 错误 + Vite 生产构建全部通过；Windows x64 release + NSIS 打包成功，启动冒烟测试通过。安装器 `智能表格_0.9.0_x64-setup.exe` 大小 4,236,153 字节，SHA-256 `A2679ACBA072D389FFEB02319A719382B65E27A0CF870BBC01FE9284D3204725`。GitHub Release [`v_13`](https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_13) 已发布。

### M33 — 原图拖拽（2026-06-16）

- 新增从缩略图直接拖拽原图文件到外部应用的功能，使用 `tauri-plugin-drag`（CrabNebula）实现原生 OS 文件拖拽。
- 后端新增 `prepare_file_drag` 命令：解析行的原图文件路径并定位缩略图缓存文件作为拖拽图标。
- 前端新增 `file-drag.ts` 工具模块：mousedown + mousemove 阈值检测区分点击与拖拽，超过 5px 移动后启动原生文件拖拽，不影响原有点击行为。
- 支持范围：画廊卡片、表格行缩略图、详情面板预览/大图、分组卡片缩略图、以图搜图结果。
- 全量验证：Rust 编译通过，`svelte-check` 0 错误，NSIS 安装包构建成功。
- 涉及文件：`Cargo.toml`、`lib.rs`、`commands.rs`（后端）；`package.json`、`api.ts`、`file-drag.ts`（新建）、`GalleryCard.svelte`、`Thumbnail.svelte`、`DetailPanel.svelte`、`GroupSectionCard.svelte`、`SearchResultsView.svelte`、`capabilities/default.json`（前端）。

### M32 — 文件管理器集成 + 失败图片去重（2026-06-16）

- 右键菜单新增「在文件管理器中打开」：画廊、表格、分组视图的行级右键菜单均可使用，后端通过 `show_item_in_explorer` 命令解析行的实际图片路径（优先原路径、回退受管副本），调用系统文件管理器选中该文件。
- 工具菜单新增「打开失败图片目录」：通过 `open_rejected_images_directory` 命令在文件管理器中打开已配置的异常图片目录。
- 失败图片去重：`move_rejected_image` 在移动前检查目标位置是否已存在内容相同的文件（先比较文件大小，再比较字节），若已存在则跳过移动并删除源文件，避免重复导入时积累大量同内容的 `_2`、`_3` 后缀文件。
- 跨平台支持：Windows 使用 `explorer /select,`，macOS 使用 `open -R`，Linux 使用 `xdg-open`。
- 全量验证：Rust 152 项测试通过，`svelte-check` 0 错误。
- 涉及文件：`commands.rs`、`runtime.rs`、`export_images.rs`、`import_images.rs`、`storage/mod.rs`、`lib.rs`（后端）；`api.ts`、`ContextMenu.svelte`、`TopBar.svelte`（前端）。

### M31 — 搜索功能（2026-06-15）

- 顶栏新增搜索框，300ms 防抖输入即搜，带清除按钮。
- 后端 `RowQuery` 和 `RowSelection::Filtered` 新增 `search` 字段，`populate_filtered_rows` 对 `image_path`、`positive_prompt`、`negative_prompt`、`artists` 四个字段做大小写不敏感子串匹配（`INSTR(LOWER(...))`），空搜索无额外 SQL 开销。
- 搜索与 Tag 筛选叠加（AND 关系），影响画廊、表格、分组、重复所有视图及选择/导出范围。
- 全量验证：Rust 152 项测试通过，Clippy 无警告，`svelte-check` 0 错误，Vite 生产构建通过。
- 版本提升至 0.8.0，安装器 `Smart-Spreadsheet_0.8.0_x64-setup.exe` 大小 4,197,845 字节，SHA-256 `23E651A805B81CA40C945801018071B1C2C6CCC1A05114583A5713D23B9B9F18`。
- GitHub Release [`v_10`](https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_10) 已发布。
- 涉及文件：`query.rs`、`tags.rs`、`commands.rs`、`runtime.rs` 及 6 个测试文件（后端）；`api.ts`、`row-store.svelte.ts`、`selection-store.svelte.ts`、`export-actions.ts`、`TopBar.svelte`、`GroupBrowseView.svelte`（前端）。

### M30 — 分组/重复视图右键菜单（2026-06-15）

- **Section header 右键菜单**：分组视图支持"重命名"和"删除分组"，重复视图支持"重命名"（设置别名）。
- **缩略图卡片右键菜单**：`GroupSectionCard` 新增右键触发现有 ContextMenu（复制 Prompt、导出原图、删除）。
- **重复 cluster 别名系统**：新增 DB v6 迁移创建 `dedupe_aliases` 表，`set_dedupe_alias` API 支持 upsert/delete，`list_dedupe_clusters` 查询 LEFT JOIN 别名表。DuplicateBrowseView 优先显示别名，括号内附原始 key。
- 新增 `SectionContextMenu.svelte` 和 `section-context-menu.svelte.ts` 管理 section 级右键状态。
- 涉及文件：`migrations.rs`、`query.rs`、`mod.rs`、`runtime.rs`、`commands.rs`、`lib.rs`、`api.ts`、`GroupSectionCard.svelte`、`GroupBrowseView.svelte`、`DuplicateBrowseView.svelte`、`Workspace.svelte`、`SectionContextMenu.svelte`（新建）、`section-context-menu.svelte.ts`（新建）。

### M29 — 重复浏览视图（2026-06-15）

- 后端新增 `list_dedupe_clusters` 和 `get_dedupe_cluster_members` 两个数据库方法及对应 Tauri 命令，按画师串或正向提示词聚合重复项（count >= 2），支持 Tag 筛选、分页。
- 新增 `DuplicateBrowseView.svelte`：顶部模式切换（按画师串/按正向提示词），折叠式分组列表，展开后懒加载成员缩略图网格，复用 `GroupSectionCard` 组件。
- `ViewMode` 新增 `"duplicates"`，TopBar 新增"重复"视图按钮，Workspace 新增条件渲染分支。
- `api.ts` 新增 `DedupeCluster` 接口和两个 invoke 函数。
- 涉及文件：`db/query.rs`、`db/mod.rs`、`app/runtime.rs`、`app/commands.rs`、`lib.rs`、`api.ts`、`app-state.svelte.ts`、`TopBar.svelte`、`Workspace.svelte`、`DuplicateBrowseView.svelte`（新建）。

### 后续修复（2026-06-15）

- 负向提示词编辑：DetailPanel 新增负向提示词的编辑/保存/取消交互，后端新增 `update_negative_prompt` 命令（不触发画师重提取）。
- 编辑后原位更新：正向/负向提示词保存后使用 `patchRowFields` 原位更新缓存行，不再调用 `resetRows` 导致活动行丢失和画廊跳转。
- 后端 `update_positive_prompt` 返回值改为 `SinglePromptEditResult`（含 `new_artists`），前端同步更新画师串字段。
- 涉及文件：`db/prompt_edit.rs`、`db/mod.rs`、`app/runtime.rs`、`app/commands.rs`、`lib.rs`、`api.ts`、`row-store.svelte.ts`、`DetailPanel.svelte`。

### M22 — Schema v5 + 分组 CRUD 后端（2026-06-15）

- Schema v5 迁移：新增 `groups` 表（id, name UNIQUE, created_at）和 `rows.group_id` 可空外键（`ON DELETE SET NULL`），部分索引 `idx_rows_group_id`。
- 新增 `db/groups.rs`：`create_group`、`rename_group`、`delete_group`、`delete_empty_groups`、`list_groups`、`assign_rows_to_group`、`ungroup_rows`，复用 `create_selection_rows` 临时表模式。
- 查询层扩展：`RowQuery` 新增 `group_view` 和 `hide_grouped` 布尔字段（serde 默认 false）；`RowRecord` 新增 `group_id` 和 `group_name`（LEFT JOIN groups）。
- `populate_filtered_rows` 新增两个分支：`hide_grouped=true` 时追加 `AND rows.group_id IS NULL`；`group_view=true` 时按 group_id 聚合已分组行 + UNION ALL 未分组行。
- 新增 `get_group_members(group_id, offset, limit)` 分页查询命令。
- 8 个 Tauri 命令注册：create/rename/delete/delete_empty/list groups、assign/ungroup rows、get_group_members。
- 前端 TypeScript 类型同步：`RowRecord` 增加 `groupId`/`groupName`，`RowQuery` 增加 `groupView`/`hideGrouped`，`api.ts` 新增 `GroupSummary` 接口和 8 个分组 API 函数。
- `row-store.svelte.ts` 新增 `groupView`/`hideGrouped` 状态字段和 `setGroupView`/`setHideGrouped` setter（含互斥逻辑）。
- 全量验证：Rust 134 项测试通过（含 12 项分组新测试）、Clippy 无警告、`svelte-check` 0 错误、Vite 生产构建通过。

### M23 — 提示词编辑后端（2026-06-15）

- `db/prompt_edit.rs` 实现三个方法：
  - `update_positive_prompt(row_id, new_prompt)`：单行编辑正向提示词，自动重新提取画师串。
  - `find_replace_prompt(selection, find, replace)`：批量查找替换，逐行重提取画师串；空 find 为 noop。
  - `prepend_artist(selection, artist_name)`：批量添加 `artist:xxx, ` 前缀，逐行重提取画师串；空名称为 noop。
- 所有方法在 IMMEDIATE 事务内完成，使用 `extract_artist_tags()` 重算 artists 列（无画师时置 NULL）。
- 3 个 Tauri 命令注册：`update_positive_prompt`、`find_replace_prompt`、`prepend_artist`。
- 前端 `api.ts` 新增 `PromptEditResult` 接口和 3 个 API 函数。
- 全量验证：Rust 142 项测试通过（含 8 项提示词编辑新测试）、Clippy 无警告、`svelte-check` 0 错误、Vite 生产构建通过。

### M24 — 相似度引擎（2026-06-15）

- 新增 `pipeline/similarity.rs`：Token Jaccard（提示词）和 Jaro-Winkler（画师串）两种相似度度量，Union-Find 贪心聚类。
- `suggest_groups(rows, mode, threshold, progress)` 管线：先按精确 lowercase 键分桶减少比较量，再 O(n²) 桶间比较，超阈值建边聚类，输出建议分组列表。
- `ungrouped_keys(mode)` 数据库方法：按画师串或正向提示词查询所有未分组行。
- `suggest_groups` 异步 Tauri 命令 + `group-suggest://progress` 进度事件。
- 前端 `api.ts` 新增 `SimilarityMode`/`SuggestedGroup` 类型和 `suggestGroups` 函数。
- 添加 `strsim = "0.11"` 依赖。
- 全量验证：Rust 152 项测试通过（含 10 项相似度新测试）、Clippy 无警告、`svelte-check` 0 错误、Vite 生产构建通过。

### M25 — 提示词编辑前端（2026-06-15）

- `DetailPanel.svelte`：正向提示词字段新增「编辑」按钮，点击切换为 `<textarea>` + 保存/取消按钮，保存调用 `updatePositivePrompt` 后自动刷新画廊。
- 新增 `PromptEditDialog.svelte`：模态对话框含「查找替换」和「添加画师前缀」两个 tab 页，操作完成后刷新画廊并显示影响行数。
- `SelectionBar.svelte`：新增「编辑提示词」按钮，打开 PromptEditDialog。
- `svelte-check` 0 错误、Vite 生产构建通过。

### M28 — 验收与发布（2026-06-15）

- 全量门禁通过：Rust 152 项测试 + 2 项集成测试全部通过，Clippy 无警告，`svelte-check` 0 错误，Vite 生产构建通过。
- 版本统一提升至 0.7.0：`package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock` 与 `tauri.conf.json` 已同步。
- Windows x64 release + NSIS 打包成功。
- 安装器 `Smart-Spreadsheet_0.7.0_x64-setup.exe` 大小 4,176,294 字节，SHA-256 `8E8D5ADCC5A5B5FCE92A0B3AC888289D326D97A02A05ABD58465D064D4512EBF`。
- GitHub Release [`v_9`](https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_9) 已发布，资产为 `Smart-Spreadsheet_0.7.0_x64-setup.exe`。

### M27 — 分组浏览视图（2026-06-15）

- `TagSidebar.svelte`：筛选面板新增「分组视图」和「隐藏已分组」两个勾选项。分组视图开启时，去重选项和隐藏已分组均禁用（互斥逻辑由 `setGroupView` 处理）。
- 新增 `GroupBrowseView.svelte`：分组视图主界面，取代画廊/表格。按 `groupStore.list` 渲染折叠式分组 section，展开时通过 `getGroupMembers` 按需加载成员缩略图网格，支持分页加载更多。底部「未分组」section 通过 `queryRows(hideGrouped=true)` 加载未分组行。
- 新增 `GroupSectionCard.svelte`：分组 section 内的成员卡片，120px 缩略图 + 画师标签，点击设为当前活动行。
- `Workspace.svelte`：`rowStore.groupView` 为 true 时渲染 GroupBrowseView，替代 GalleryView/TableView。
- `svelte-check` 0 错误、Vite 生产构建通过。

### M26 — 分组管理前端（2026-06-15）

- 新增 `group-store.svelte.ts`：分组列表响应式 store（`groupStore`），包含 `loadGroups`、`createNewGroup`、`renameExistingGroup`、`removeGroup`、`cleanEmptyGroups`、`assignToGroup`、`removeFromGroup` 七个 action 函数。
- 新增 `GroupSuggestionView.svelte`：全屏浮层，配置相似度模式（画师串 Jaro-Winkler / 正向提示词 Token Jaccard）和阈值，运行分析后展示建议分组列表，支持全选/单选后批量创建分组并分配行。
- 新增 `GroupManageView.svelte`：全屏浮层，展示所有分组及成员数，支持内联重命名、删除和清理空分组。
- 新增 `GroupAssignDialog.svelte`：模态对话框，从已有分组列表中选择分配，或输入新组名即建即分配，支持取消分组。
- `SelectionBar.svelte`：新增「分组」按钮，打开 GroupAssignDialog。
- `TopBar.svelte`：工具菜单新增「建议分组」和「管理分组」两个入口。
- `DetailPanel.svelte`：正向提示词编辑（M25）+ 分组信息显示与「取消分组」按钮；清理未使用的 group-store 导入。
- `Workspace.svelte`：条件渲染 GroupSuggestionView 和 GroupManageView。
- `app-state.svelte.ts`：新增 `groupSuggestOpen` 和 `groupManageOpen` 布尔状态。
- `svelte-check` 0 错误、Vite 生产构建通过。

### v0.6.0 - 代码质量重构（2026-06-14）

- `runtime.rs`：提取 `with_database` 泛型辅助方法，消除 13 个命令处理器中重复的 lock→validate→get-directory 样板代码。
- `commands.rs`：为内部查询/存储类型添加 serde 派生，删除 12 个冗余镜像 DTO 及其 `From` 实现（文件从约 900 行缩减至约 480 行）。
- `TagSidebar.svelte`：移除与全局样式冲突的局部 `.btn` / `.btn-danger` CSS 定义。
- `app-state.svelte.ts`：导入相关函数（`chooseImageFolder`、`chooseImageArchive`、`chooseRejectedImagesDirectory`、`ensureRejectedImagesDirectory`、`runImageImport`）抽取到独立模块 `import-actions.svelte.ts`。
- 全量验证：Rust 122 项测试通过、Clippy 无警告、`svelte-check` 0 错误、Vite 生产构建通过。

## 第三轮迭代

### M19 - 显式全选（2026-06-12）

- 多选操作栏新增「全选」按钮，点击后复用 filtered 选择模型，覆盖当前 Tag 筛选、AND/OR 模式和去重显示下的全部匹配行。
- 已处于筛选全选或明确选择已覆盖全部匹配行时不重复显示按钮；`Ctrl+A` 快捷键保持不变。
- `svelte-check` 0 错误 0 警告，Vite 生产构建通过；`PLAN.md` 对应验收项已勾选。

### M20 - 异常图片隔离（2026-06-12）

- 当前资料库新增异常图片目录设置，复用现有 `settings` 表持久化，不提升 schema 版本；应用快照同步返回已配置路径。
- 首次文件夹/压缩包图片导入前自动要求选择目录；工具菜单新增「异常图片目录」入口，可随时更改并显示当前目录名。
- 后端创建并规范化用户目录，拒绝文件路径及受管数据目录内部路径；新增设置读写自动化测试。
- 图片导入顺序改为「身份筛选 → metadata 检查 → 正常图内容哈希 → 入库/副本落位 → 异常图移动」；读取失败或正负提示词均为空时不再生成数据库候选。
- 文件夹、单 PNG 与压缩包异常图均按来源内相对路径移动；目标同名自动追加 `_2` 等编号且不覆盖，跨盘时复制成功后再删除源文件。
- 移动失败不回退入库，结果分别报告异常总数、移动成功和移动失败；前端导入提示同步更新。
- 自动化覆盖空 `Description`、无提示词 `Comment`、损坏 PNG、文件夹相对目录、单 PNG、压缩包受管副本、重名保护与移动失败。
- 完整验证：Rust 117 项单元测试与 2 项集成测试通过，Clippy 无警告，`svelte-check` 0 错误 0 警告，Vite 生产构建通过；`PLAN.md` 第 13.3 节五项验收全部勾选。

### M21 - 验收（2026-06-12）

- Windows x64 release 编译与 NSIS 打包成功，安装器输出为 `target/release/bundle/nsis/智能表格_0.5.0_x64-setup.exe`。
- 安装器大小 4,306,269 字节，SHA-256 `A753E85E08BC745CAA2C56766CF8E0EECE595B746C240F0EDC4873AF5C5D84DC`。
- 发布版使用隔离的 `APPDATA` / `LOCALAPPDATA` 启动，进程 4 秒保持运行，启动冒烟测试通过；临时目录已清理。
- 当前 `main` 已包含 M19 显式全选、异常目录持久化和 M20 空 metadata 图片隔离的分步提交，均已推送 GitHub。
- 发布准备：`package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock` 与 `tauri.conf.json` 已统一提升至 0.5.0。
- 发布前全量门禁再次通过：Rust 117 项单元测试与 2 项集成测试、Clippy、`svelte-check` 和 Vite 生产构建全部正常。
- GitHub Release [`v_5`](https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_5) 已发布，资产为 `Smart-Spreadsheet_0.5.0_x64-setup.exe`，远端大小与 SHA-256 均和本地一致，更新说明按约定使用一句话。

### v0.5 需求对齐（2026-06-12）

- 多选操作栏新增显式「全选」入口，作用于当前 Tag 筛选和去重显示后的全部匹配行，保留 `Ctrl+A`。
- 文件夹/单 PNG/压缩包导入中，读取失败或正负提示词均为空的图片不入库；移动到当前资料库保存的用户自定义异常图片目录，保留相对路径且不覆盖同名文件。
- 详细规则与验收清单见 `PLAN.md` 第 13 节。

## 已完成

### M15 - 导入内容去重（2026-06-12）

- 完成 schema v2→v3 迁移：`rows` 新增可空 `content_hash TEXT`，并新增允许重复值的部分索引 `idx_rows_content_hash`；历史行迁移后保持 `NULL`，等待打开受管目录时补算。
- 新增迁移测试，覆盖全新 v3 建库、v1→v3 和 v2→v3；确认行顺序、提示词、Tag 关联和待提取嵌入图记录无损，且哈希索引不是唯一约束。
- 打开受管数据目录时自动补算历史行 SHA-256：严格按原图片路径优先、受管副本回退读取原始文件字节；无法读取的行保留 `NULL` 且不阻塞打开。补算提供逐行进度（总数/已处理/已更新/不可读），数据库批量回填在单事务内完成。
- 新增标准 SHA-256 向量、数据库事务回填和真实数据目录回填测试，覆盖外部图、压缩包受管副本回退及不可读行。
- 追加批次事务接入内容哈希约束：严格先判身份键，再判库内及本批次已见内容；新增 `skipped_content` 独立计数，批次总跳过数包含两类跳过；哈希为 `NULL` 的候选不参与内容去重。
- 文件夹/单 PNG/压缩包导入改为“身份筛选 → 并行 SHA-256 → 内容筛选 → 元数据解析/副本落位”：内容重复项不读取元数据、不复制压缩包副本；前端新增哈希检查进度和「内容重复跳过 N」独立结果提示。
- 自动化验收覆盖 500 份同内容图片只入库 1 行、跨文件夹同内容跳过、压缩包内重复只保留 1 份受管副本，以及 10,000 行库追加 5 份不同图片仍得到 10,005 行。
- xlsx 导入接入同一内容去重规则：G 列外部图片可读时哈希原文件，否则哈希提取后的嵌入图；内容重复行不入库并清理其暂存嵌入图，结果独立报告「内容重复跳过 N 行」。
- 自动化覆盖外部图片优先/嵌入图回退，以及先从另一目录导入同内容图片后再导入样表（新增 4、内容跳过 1、仅保留 4 份嵌入副本）。
- 打开已有受管目录改为异步阻塞任务，历史哈希补算通过 `content-hash://progress` 实时展示已处理、已更新和不可读计数；大库升级不再表现为设置页无反馈卡顿。
- `PLAN.md` 12.6 中 M15 三项验收全部勾选。Rust 全量测试 109 项通过；Clippy 无警告；`svelte-check` 0 错误 0 警告，前端生产构建通过。

### M16 - 通用删除 + 去重显示（2026-06-12）

- 查询与 filtered 选择协议新增互斥去重维度：无 / 正向提示词 / 画师串。后端先应用 Tag AND/OR 筛选，再对 trim 后非空键按入库顺序保留首行；空键行全部保留，大小写敏感。
- 分页、`N 条匹配`、Ctrl+A 全选、批量 Tag、删除与三种导出共享同一临时代表行集合；排除行只移除当前代表，不会隐式提升同组下一行。
- 自动化覆盖正向提示词/画师串去重、空键保留、大小写敏感、Tag 先筛选后去重，以及 filtered 选择仅作用于可见代表行。
- 筛选区新增「按正向提示词去重 / 按画师串去重」两个互斥勾选项；切换时重载画廊/表格并清空旧选择，匹配计数和后续批量操作同步使用当前去重维度。
- 退役旧「库内查重」浮层、工具菜单入口、`find_duplicates` Tauri/API/运行时/数据库链路及专用测试；智绘姬 JSON 去重工具保持不变。
- 通用删除后端新增可选原图回收站处理：数据库事务先删除库行，再通过 `trash` 将文件夹/单 PNG 来源原图移入系统回收站；失败仅计数、不回滚库行，压缩包来源按无独立原文件单独统计跳过。受管副本与缩略图清理语义保持不变。
- 删除命令结果新增成功回收原图、原图回收失败、压缩包来源跳过三项计数；自动化覆盖原图候选提取、成功/失败汇总、压缩包跳过及未勾选时不触碰原图。
- 新增自定义删除确认弹窗，默认不勾选原图回收；结果提示汇总库行删除、原图回收成功/失败、压缩包来源跳过和受管文件清理失败计数。
- 删除入口覆盖选择栏按钮、详情面板单行按钮与 `Delete` 快捷键；快捷键优先作用于当前选择，无选择时作用于详情行，并避开输入框、按钮等交互控件。
- `PLAN.md` 12.6 中去重显示与通用删除验收项已勾选。当前 Rust 全量测试 111 项；Clippy 无警告；`svelte-check` 0 错误 0 警告，前端生产构建通过。

### M17 - Tag 交互方案 B（2026-06-12）

- 新增选中范围 Tag 覆盖查询协议：复用 explicit / filtered（含 Tag 条件、去重维度、排除行）选择模型，在数据库临时目标行集合上返回每个 Tag 的选中行覆盖数，支持前端准确显示全有 / 部分 / 无。
- 新增选择物化协议：进入打标模式前可将 filtered 全选稳定解析为有序行 ID，防止贴/解除筛选所用 Tag 后“当前选中行”随查询结果漂移。
- 侧边栏重构为「筛选 / 打标」双模式：筛选模式保留 Tag AND/OR、清除筛选和两种去重显示；打标模式按当前选择显示每个 Tag 的全有 / 部分 / 无状态，点击在全部贴上与全部解除之间切换。
- 打标模式提供「即建即贴」输入框，不存在的 Tag 由批量添加事务创建并贴到全部选中行；无选择时禁用并显示指引。详情面板输入框同步支持不存在的 Tag 回车即建即贴。
- 选择栏原批量加/减 Tag 弹窗与侧边栏独立新建表单已退役，选择栏仅保留删除与清除。
- `PLAN.md` 12.6 中 Tag 交互验收项已勾选。自动化覆盖显式选择与筛选全选（含排除行）的覆盖计数、零覆盖 Tag 和 filtered 选择物化；当前 Rust 全量测试 113 项，Clippy 无警告；`svelte-check` 0 错误 0 警告，前端生产构建通过。

### M18 - 验收与发布（2026-06-12）

- `PLAN.md` 12.6 六项 v0.4 验收标准已全部勾选；最终全量验证为 Rust 113 项测试通过、Clippy 无警告、`svelte-check` 0 错误 0 警告、Vite 生产构建通过。
- 版本统一提升至 0.4.0：`package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock` 与 `tauri.conf.json` 已同步。
- Windows x64 release + NSIS 构建通过，发布版在隔离的 `APPDATA` / `LOCALAPPDATA` 下启动冒烟测试通过，临时目录已清理。
- 安装器 `智能表格_0.4.0_x64-setup.exe` 大小 4,304,795 字节，SHA-256 `D035609BA2F65E13D3F0F166E728B521549350287DD96479B36A7577F5E1B1AB`。
- GitHub Release [`v_4`](https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_4) 已发布，资产为 `Smart-Spreadsheet_0.4.0_x64-setup.exe`，更新说明按约定使用一句话。

### 第二轮迭代需求对齐（2026-06-12）

- 用户 v0.3 实机反馈四项偏差，经问答逐项对齐：
  1. 导入按内容（SHA-256）**全库**去重：本次导入内与库内已有行均比对，同内容只留一张；旧库打开时补算哈希。
  2. 「库内查重」浮层退役，改为画廊/表格的「按正向提示词 / 按画师串去重」互斥勾选项（只影响显示，每组留一张代表图）。
  3. 新增通用删除（选择栏按钮 / 详情面板 / Delete 键），自定义确认弹窗可勾选「同时将原图移入回收站」（默认不勾）；压缩包来源行无独立原文件，勾选时跳过并说明。
  4. Tag 交互按方案 B 重做：侧边栏「筛选 | 打标」双模式，打标模式点 Tag 直接对选中行贴/解除、输入框即建即贴；选择栏批量加/减弹窗与侧边栏新建表单退役；详情面板编辑保留并支持即建即贴。
- 详细规格与验收清单见 `PLAN.md` 第 12 节。

### M14 - 验收与发布（2026-06-12）

- 万行级增量导入自动化验收：10,000 行库追加导入 5 张 PNG 文件夹 → 10,005 行；重复导入同一文件夹全部跳过、行数不变（测试 `appends_five_images_to_ten_thousand_row_library`，0.07s 完成）。
- v1 旧数据目录真实应用升级验收：用 v0.3.0 release 应用打开 v_2 时代的真实 v1 受管目录（`D:\AAA中转站\智能表格test`，升级前已备份到 `D:\Agent\Agent_temp\智能表格test-v1-backup`）。验证 schema 升至 v2、旧工作簿转为批次 1（xlsx，5 行）、行 ID 与源行号保持、Tag「原神」「爱意」各 3 行关联完整、5 张嵌入图全部提取到 `files/1/embedded/row-N.png` 且行记录回填、`pending_embedded_extractions` 清空；应用窗口正常显示，关闭后以 v2 目录再次启动正常。
- 版本提升至 0.3.0（package.json / Cargo.toml / tauri.conf.json）；Windows x64 release + NSIS 构建通过（bundler 临时目录定向 `D:\Agent\Agent_temp`，NSIS 工具复用已有缓存）。
- 安装器 `智能表格_0.3.0_x64-setup.exe` 大小 4,185,002 字节，SHA-256 `06B0B9625EA64633C61AB75F4187DFF167552D53D7BADCD88CE5E5272DAD5F94`。
- Novelai工具 仓库已新增 README 标注「已并入 Smart-Spreadsheet」并推送（仅新增 README，未触碰该仓库其他文件）。
- 测试总计 100 项通过；Clippy 无警告；`svelte-check` 0 错误。

### M13 - 导出体系 + JSON 去重页（2026-06-12）

- 删除旧版 OOXML 补丁导出（`excel/export.rs`、`storage/export.rs`、`workbook` 副本依赖），xlsx 导出改为 `rust_xlsxwriter` 全新生成：8 列（图片缩略图嵌入单元格居中、时间、正负提示词、画师串、图片文件夹、图片路径、Tags），表头冻结、列宽行高预设，文本超长按 Excel 上限截断；个别图片不可用时该行仍导出文字字段并计数。拒绝覆盖已有目标，临时文件写完改名归位。
- 新增智绘姬 JSON 导出：按入库顺序流式写出连续编号 presets（fixedPrompt / fixedPrompt_end / negativePrompt），顶层 `images` 为空对象；写临时文件后原子替换，允许覆盖目标。
- 新增图片文件导出：在所选文件夹下新建 `智能表格图片导出[_N]` 输出目录，文件平铺命名 `00001_原名.png`（库内顺序前缀，替代 PLAN 原 imageN 子文件夹设计——扁平命名排序一致且更便于直接浏览）；来源优先原路径、其次受管副本，支持复制或 NTFS 硬链接（失败自动回退复制），缺图行计数返回。
- 三种导出共享行快照查询 `export_rows`（选择模型 explicit / filtered 复用 Tag 服务基础设施），均为异步命令 + `export://progress` 进度事件；导出范围 = 有勾选导勾选、无勾选导当前筛选。
- 自 Novelai工具 原样移植智绘姬 JSON 去重（依赖 serde_json `preserve_order` 保持预设顺序）：检查（总数/重复/去重后 + 前 3 条预览）与执行（重新连续编号、临时文件原子写出、禁止输入输出同路径）两个命令，进度经 `json-dedupe://progress` 推送。新增共享 `fsx` 文件工具模块（扩展名校验、唯一同级临时路径、原子替换、临时文件守卫）。
- 前端：顶栏重构为 工具▾（库内查重 / 智绘姬 JSON 去重 / 迁移数据目录）、导入▾、导出▾（primary）三个下拉菜单（新增 Dropdown 组件），导出菜单项实时显示范围提示（已选 N 行 / 当前筛选结果 / 全部行）；新增智绘姬 JSON 去重浮层（选文件→统计预览→另存为去重）；进度 toast 泛化支持导出进度。
- 验证：Rust 99 项测试通过（含 xlsx 导出后 calamine 重读 Tags 列与固定结构再解析、JSON 转义与原子替换、图片导出顺序命名与缺图计数、输出目录自动编号）；Clippy 无警告；`svelte-check` 0 错误；前端生产构建通过。

### M12 - 库内查重视图（2026-06-12）

- 新增 `find_duplicates` 查询：按正向提示词或画师串精确分组（裁剪首尾空白、区分大小写、空值不参与），SQL 聚合 + 临时键表索引完成，不在前端计算；返回全库组数与多余行数，并按首行入库顺序返回前 N 组（含每行 Tag）。
- 前端新增查重视图（顶栏"查重"入口，全屏浮层）：分组依据切换、组内缩略图卡片勾选、"保留第一行"单组/全局快捷操作、底部删除栏。
- 删除保护：任何一组被全部勾选时禁止执行并提示"每组必须至少保留一行"；删除前弹确认框，说明原始图片文件不受影响。
- 删除复用 M10 删行链路（事务 + 副本/缩略图清理），完成后刷新查重报告、资料库摘要并触发数据视图整体重载。
- 验证：Rust 95 项测试通过（含大小写敏感分组、画师串分组带 Tag、组数上限、无重复空报告）；`svelte-check` 0 错误；前端生产构建通过。

### M11 - 提取管线移植 + 文件夹/压缩包导入（2026-06-12）

- 自 Novelai工具 移植可复用管线到 `pipeline/` 模块：PNG 文本 chunk 读取器（Seek 跳过像素数据）、NovelAI 元数据解析（Description/Comment/v4 caption）、画师片段提取、zip/7z/rar 解压（`zip`/`sevenz-rust`/`unrar-ng` 静态链接）、有序并行映射（线程数跟随可用并行度，上限 32）。连同其测试一并移植；anyhow 错误统一改为 thiserror。
- 未移植（按合并决策废弃）：`.novelai_metadata_cache` 增量缓存、输出包/时间前缀目录、导入期去重分组。扫描时仍会跳过旧版工具遗留的缓存目录和输出包目录，避免把导出副本当原图导入。
- 新增 `import_images` 导入编排：文件夹/单 PNG 直接扫描，压缩包先解压到 `D:\Agent\Agent_temp` 运行临时目录（结束清理）；身份键与库比对后只为新图读元数据；压缩包新图副本移动到受管暂存目录，入库事务内随批次 ID 归位 `files/<批次>/`。
- 已入库图片跳过时不读取任何像素或元数据；元数据解析失败的图片仍入库并带失败标记；"图片路径"列记录原路径或"压缩包 > 包内路径"溯源；时间取文件创建时间（本地时区格式化）。
- 新增异步 `import_images` Tauri 命令（spawn_blocking 不阻塞 UI），进度经 `import-images://progress` 事件按 ≥100ms 节流推送（解压/扫描/处理三阶段）。
- 前端：顶栏与首次导入页新增"导入文件夹/导入压缩包"入口，底部进度条横幅展示阶段与百分比，完成提示报告发现/新增/跳过/变化/失败计数。
- 验证：Rust 94 项测试全部通过（含文件夹追加导入、zip 副本落位与字节一致、重复导入全跳过、删除后可重新导入、进度事件、空输入报错）；Clippy 无警告；`svelte-check` 0 错误；前端生产构建通过。

### M10 - 数据模型改造（2026-06-12）

- schema v2：`workbook` 单例表替换为 `import_batches` 批次表；`rows` 增加批次 ID、`identity` 唯一身份键（增量跳过依据）、源文件大小/修改时间、受管副本路径和元数据失败标记；行排序从源行号改为入库顺序（`rows.id`）。
- 身份键规则（`db/identity` 模块，与迁移 SQL 一致）：文件夹图 = 规范化绝对路径；压缩包图 = 压缩包路径+包内路径；xlsx 行 = 图片路径列（为空或批内重复时退化为 小写文件名+源行号）。路径规范化为 trim + `/`→`\` + ASCII 小写。
- v1→v2 迁移：单事务重建表（外键临时关闭，行 ID 不变，Tag 关联无需迁移），旧工作簿转为批次 1；嵌入图引用先进 `pending_embedded_extractions`，打开数据目录时从工作簿副本批量提取到 `files/1/embedded/` 并回填行记录；副本缺失或损坏时降级清空，不阻塞打开。
- 追加导入 API：`append_batch` 单事务完成批次写入与身份跳过，返回新增/跳过/变化计数；`finalize` 回调在提交前归位批次文件目录，失败整体回滚。验证 10 行库导入 5 张图（3 旧 2 新）得 12 行，重复导入不翻倍。
- 删除行：选择模型复用 Tag 服务基础设施，级联清理 Tag 关联、受管副本文件和该行缩略图缓存；Tag 定义保留；被删身份键可重新导入。
- xlsx 导入改为追加语义：不再复制工作簿到受管目录，新行的嵌入图一次性提取到 `files/<批次>/embedded/`（ZIP 只打开一次），重复导入同一文件全部跳过且 Tag 保留。
- 图片回退链改为：原路径 → 受管副本 → 占位；旧版 OOXML 导出保留但仅对“迁移遗留的单一 xlsx 批次”可用，其余情况报明确错误（M13 替换为生成式导出）。
- 前端适配：快照改为资料库摘要（行数/批次数/最近导入），导入按钮改为追加语义并报告新增/跳过/变化/提取计数，新增 `delete_rows`、`list_import_batches` 命令通道。
- 验证：Rust 75 项测试全部通过（含 v1→v2 迁移保 Tag 保序、追加跳过、删除清理、迁移后再导入）；Clippy 无警告；`svelte-check` 0 错误 0 警告；前端生产构建通过。

### 合并决策（2026-06-12）

- 与用户对齐：将 `D:\Agent\Novelai工具`（PNG 元数据提取 → xlsx）并入本项目，xlsx 从必经中间件降级为可选导入/导出格式，Novelai工具 退役。
- 四项关键决定：追加式资料库（增量导入、已入库自动跳过）；输出包功能转为可选导出；智绘姬 JSON 升级为直接从库导出 + JSON 去重页移植；导入去重改为库内查重视图（含删除行能力）。
- 用户确认的核心场景：10000 行库导入 5 张图压缩包 → 10005 行；重复导入不翻倍；压缩包图片提取副本到受管目录，删压缩包不影响浏览。
- 按新蓝图重写 `PLAN.md`（数据模型 v2、导入管线移植清单、M10–M14 里程碑与验收标准）。

### M0 - 项目初始化

- 审阅任务说明、原 `PLAN.md` 和示例 Excel。
- 确认样表包含 1 个工作表、5 条数据和 5 张按行锚定的嵌入式 PNG。
- 确认样表 G 列的 5 个本地图片路径当前均有效。
- 与用户确认第一版产品边界：固定 NovelAI 表格结构、单表、仅编辑 Tag、AND / OR 筛选、完整批量操作、万行级数据。
- 确认原 Excel 永不修改，应用独立持久化，并提供导出副本功能。
- 确认应用数据目录支持完整迁移，而不是只更换路径。
- 重写 `PLAN.md`，移除硬编码 Excel 路径、直接写回原表及完整重写工作簿等不合适方案。
- 新建 `.gitignore`，排除构建产物、运行数据库、缓存、日志和本地环境文件。
- 初始化 Git `main` 分支，配置并推送 GitHub 远端。

### M1 - 技术验证

- 建立 Rust Cargo 工作区和 `src-tauri` 核心库基础结构。
- 使用 `calamine` 实现固定结构 Excel 只读解析器，按表头名称定位 7 个必需字段，不依赖固定列号。
- 支持跨工作表查找匹配结构、重复表头报错、缺失表头汇总和空数据行过滤。
- 保留源 Excel 行号，为后续 Tag 导出回填建立稳定映射。
- 添加样例集成测试，确认 5 条数据对应源行 2–6，且解析前后源文件字节完全不变。
- 实现 workbook、worksheet、drawing 三层 OOXML 关系解析，支持 `oneCellAnchor` 和 `twoCellAnchor` 起始坐标。
- 样例中的 5 张嵌入式 PNG 已正确映射到 A2–A6，并可通过媒体部件路径按需读取原始字节。
- 图片映射测试同时确认媒体 PNG 签名有效，且读取前后源 Excel 字节完全不变。
- 抽出共享 OOXML 关系解析层，供图片读取和导出共同使用。
- 实现最小 Tag 导出器：通过内联字符串在最后一列写入 `Tags`，其余 ZIP 部件使用压缩数据原样复制。
- 导出使用目标目录内的临时文件，完整写入并同步后再原子重命名；失败时清理临时文件，且不覆盖已存在目标。
- 自动化保真测试确认导出文件可由 `calamine` 重新读取，大小写 Tag、中文和 XML 特殊字符均正确。
- 自动化保真测试确认除目标工作表 XML 外的所有 ZIP 部件内容不变，嵌入图片映射及图片字节保持一致，源 Excel 字节不变。
- 使用本机 Microsoft Excel 只读打开导出样例，确认 `Tags` 列内容正确、5 张图片仍在，打开前后导出文件 SHA-256 不变。

### M2 - 应用数据层

- 引入 bundled SQLite，避免依赖目标机器预装 SQLite 运行库。
- 建立 schema v1：`workbook`、`rows`、`tags`、`row_tags`、`settings`，并启用严格表、外键和必要索引。
- 使用 `PRAGMA user_version` 实现原子版本迁移，拒绝打开高于应用支持版本的数据库。
- 数据库连接启用外键、WAL、`synchronous=NORMAL` 和 5 秒 busy timeout。
- 验证 `Landscape` 与 `landscape` 可同时存在，完全相同的 Tag 会触发唯一约束。
- 验证删除工作簿会级联删除行和行标签关联，持久数据库可重开且不会重复执行迁移。
- `rusqlite` 固定为 `0.39.0`；最新版 `0.40.1` 的 `libsqlite3-sys 0.38.1` 构建脚本使用当前稳定 Rust 未开放的 `cfg_select`，因此未升级本机 Rust。
- 实现版本化受管数据目录，固定包含数据库、`workbook/`、`cache/thumbnails/` 和 `migration/`。
- 使用 `.smart-spreadsheet-data.json` 标记目录所有权和格式版本，拒绝占用普通非空文件夹或打开未来版本目录。
- 数据目录初始化可幂等重开，并会验证必要路径、SQLite 文件和 schema 可用性。
- 实现完整数据目录迁移：普通文件递归复制并逐文件做字节校验，SQLite 使用在线备份 API 生成一致快照并执行完整性检查。
- 迁移在目标同级暂存目录中完成，校验通过后再改名为正式目录；失败时删除暂存目录并继续使用原目录。
- 切换前将旧目录标记移入其 `migration/`，确保旧目录即使清理失败也不再是可打开的并行工作区。
- 拒绝迁移到非空目录、当前目录本身或当前目录内部；安全检查完成前不创建目标父目录。
- 自动清理旧目录，但不会自动递归删除文件系统根目录下的一级目录，此时返回待人工清理路径。
- 实现工作簿事务导入：先在受管 `workbook/` 目录创建独占暂存文件，复制、同步并做字节校验后再解析。
- 导入时将固定结构元数据、源 Excel 行号和 A 列嵌入图片引用一次性写入 SQLite。
- 替换工作簿时数据库使用单事务；任一行失败会恢复此前工作簿与 Tag 数据。
- 内部工作簿副本交换失败或数据库写入失败时恢复旧副本；源 Excel 在成功和失败路径中均不修改。
- 损坏的替换文件会在交换前被拒绝，已有内部副本和数据库保持不变。

### M3 - 查询与 Tag 服务

- 实现基于数据库行 ID 的单行和批量 Tag 添加、删除 API。
- Tag 输入去除首尾空白、跳过空字符串、按精确大小写去重，保留中间字符和原始大小写。
- 批量操作先将目标行写入 SQLite 临时表并验证全部存在，再在 `IMMEDIATE` 事务内修改关联，避免部分成功。
- 重复添加已有 Tag 关联为幂等操作；删除后自动清理不再被任何行使用的 Tag。
- 验证未知行会使整个批次回滚，且同一数据库连接可在回滚后继续正常操作。
- 验证 10,000 行批量添加和删除不依赖 SQLite 参数数量拼接，并能一次事务完成。
- 实现稳定分页查询，按源 Excel 行号排序，单页限制为 1–500 行，并返回筛选总数和 `has_more` 状态。
- 页面元数据和页面 Tag 分两次批量查询完成，不执行每行一次 SQL，也不向前端发送未请求的长文本记录。
- 使用 SQLite 临时表保存筛选 Tag，避免 Tag 数量受动态 SQL 参数拼接限制；筛选输入沿用精确大小写和去空白规则。
- 实现全局 AND / OR 筛选：AND 要求包含全部选中 Tag，OR 要求包含任一选中 Tag，空筛选返回全部行。
- 实现已使用 Tag 聚合，返回每个精确大小写 Tag 的关联行数，并按二进制顺序稳定排序。
- 验证 10,000 行数据可直接查询末尾 100 行，同时只返回该页记录并正确统计 10,000 行总数。
- 实现 `Explicit` 和 `Filtered` 两种选择模型：前者保存当前页或勾选行 ID，后者保存筛选 Tag、AND / OR 模式和排除 ID。
- 筛选结果全选、选中计数和批量 Tag 修改均在同一 SQLite 事务内解析，不要求前端保存或传输全部匹配行 ID。
- 筛选选择先物化目标行，再执行 Tag 修改；即使批量删除筛选条件本身，也不会因条件变化漏掉目标行。
- 验证 10,000 行全选只传两个排除 ID即可准确修改 9,998 行，排除行保持不变。
- 筛选无匹配结果时不会创建孤立 Tag；显式行 API 保持为选择模型的兼容封装。

### M4 - 主界面

- 搭建 Tauri v2.11.2 + Vite v8 + TypeScript v6 的 Windows 桌面应用骨架，不安装全局 Tauri CLI，不更新 Node。
- npm 依赖安装缓存临时定向到 `D:\Agent\Agent_temp\npm-cache`，避免依赖缓存占用 C 盘。
- 配置模块化前端入口、生产构建、Tauri capability 和 dialog 插件权限。
- 新增应用图标 SVG 源，并通过仓库内 Tauri CLI生成 Windows ICO 及标准平台图标。
- 完成首次设置占位界面，明确显示原 Excel 只读、Tag 大小写敏感和导出副本规则。
- 前端生产构建、Rust `cargo check`、Clippy 和全部后端测试通过。
- 完成桌面启动冒烟测试：成功创建标题为“智能表格”的响应窗口，验证后关闭测试进程。
- 实现持久化应用运行时状态：在标准应用配置目录仅保存版本化数据目录定位文件，数据库、工作簿和缓存仍全部位于用户选择目录。
- 启动时自动恢复受管数据目录和工作簿摘要；定位文件无效时保留错误状态，不自动覆盖或切换工作区。
- 首次配置支持初始化空目录或打开已有受管目录；一旦已配置，普通配置命令拒绝更换路径，后续只能通过迁移功能切换。
- 新增 Tauri commands：读取应用状态、初始化目录、打开已有目录和导入工作簿。
- 首次设置界面已接通 native 目录/文件对话框；导入固定结构 `.xlsx` 后显示工作簿名、工作表、行数和导入时间。
- 替换已有工作簿前要求用户明确确认，并说明会清除现有行与 Tag 数据但不会修改原 Excel。
- 前端对路径、文件名和后端错误文本做 HTML 转义，避免工作簿元数据进入 DOM 时被解释为标记。
- 使用 D 盘临时受管数据目录完成状态命令版桌面启动冒烟测试。
- 新增 `query_rows` Tauri command，将既有分页、Tag AND / OR 筛选和计数能力暴露给桌面前端。
- 实现固定 116px 行高的虚拟表格，按 200 行分页缓存，只渲染可视区域及 5 行过扫描范围。
- 表格支持滚动到任意分页、邻页预取、加载占位和长提示词截断，不会将全部长文本一次性发送到前端。
- 实现单行详情弹窗，可查看并复制正向提示词、负向提示词、画师串和图片路径。
- 使用浏览器模拟 10,000 行数据完成验收：首屏仅渲染 14 行，滚动到底部仅渲染 9 行并正确显示源 Excel 第 10001 行。
- 新增已使用 Tag 聚合、选择计数，以及按显式行或筛选条件批量添加/删除 Tag 的 Tauri commands。
- Tauri 选择协议保留大小写敏感筛选条件和排除行 ID，批量操作继续由数据库事务保证整体成功或回滚。
- 实现 Tag 工作区：展示已使用 Tag 及全局关联数，支持大小写敏感的多 Tag AND / OR 筛选和一键清除。
- 虚拟表格新增逐行复选框；支持跨页显式选择、当前 200 行页选择，以及“筛选条件 + 排除行 ID”的全部筛选结果选择。
- 批量编辑支持每行输入一个 Tag、前端去空白和精确去重，并可对当前选择添加或删除；删除前要求明确确认。
- 批量操作成功后会清除选择、重载当前查询并刷新已使用 Tag 计数，单行勾选同样可完成单行 Tag 编辑。
- 使用浏览器模拟 10,000 行完成交互验收：`Red + Blue` 在 AND 下匹配 1,666 行、OR 下匹配 6,667 行；当前页选择 200 行；筛选结果全选 3,333 行并排除一行后，批量添加和删除均准确变更 3,332 个关联。
- Tag 筛选和批量编辑期间虚拟表格仍只渲染可视区附近 14 行。
- 新增行图片定位查询和 `get_row_thumbnail` / `get_row_preview` 二进制 Tauri commands。
- 图片读取严格按 G 列路径优先、工作簿嵌入图回退；缺失或无法解码的外部图片不会阻断嵌入图加载。
- 缩略图限制在 256px，预览限制在 2048px，并禁止放大小图；统一编码为 PNG 二进制响应。
- 缩略图按图片来源、文件大小和修改时间生成缓存键，写入受管 `cache/thumbnails/` 时使用临时文件、同步和原子改名；来源变化后清理该行旧缓存。
- 自动化测试覆盖外部图片优先、外部路径失效后嵌入图回退、缩略图缓存复用，以及导入数据库后的运行时缩略图/预览读取。
- 虚拟表格已接通缩略图懒加载，仅在行进入可视区及过扫描范围时请求图片；无图片来源时直接显示缺失状态。
- 前端缩略图加载器限制最多 4 个并发请求、缓存最多 120 个对象 URL，并在快速滚动时取消尚未开始的旧可视区任务。
- 点击缩略图后按需请求 2048px 预览；关闭、按 Escape 或点击遮罩时均释放预览对象 URL。
- 使用 10,000 行浏览器模拟验收：首屏只请求 14 张，直接滚到底部累计请求 23 张；快速跨越四个远距离位置只启动 17 次请求，并发峰值始终为 4。
- 浏览器验收确认预览仅发起一次请求，弹窗关闭后对象 URL 被立即撤销。
- 新增全部行 Tag 导出快照查询，按源 Excel 行号输出每一行，并按二进制顺序稳定连接大小写敏感 Tag。
- 新增 `export_workbook` Tauri command，始终基于受管工作簿副本生成新的 `.xlsx`，不会依赖原 Excel 继续存在。
- 导出后端拒绝覆盖任何已有文件及受管工作簿副本，避免用户误选原 Excel；导出前后还会比较内部副本完整字节。
- 自动化测试确认含 Tag、空 Tag 和中文 Tag 的 5 行均正确写入最后一列，内部副本保持不变，已有目标内容不会被覆盖。
- 工作区新增“导出副本”入口，通过 native 保存对话框选择新的 `.xlsx` 路径，默认文件名为“原文件名-tagged.xlsx”。
- 导出期间使用统一忙碌状态；成功后显示导出行数和完整路径，取消保存不会调用后端或改变工作区。
- 浏览器模拟验收确认保存对话框标题、默认文件名、扩展名过滤器和导出路径传递均正确，完成后按钮恢复可用。
- 数据目录迁移重构为准备、提交、回滚三阶段：复制和完整性校验完成后，只有定位文件原子更新成功才会清理旧目录。
- 定位文件更新失败时会恢复旧目录标记并使目标目录失效，避免产生两个可打开的并行工作区。
- Windows 迁移路径在安全校验后移除 `\\?\` 系统前缀，定位文件和界面均显示用户熟悉的盘符路径。
- 工作区新增“迁移数据目录”入口，明确说明空目录、完整迁移、失败继续使用旧目录；迁移后会更新路径或提示未自动清理的旧目录。
- 自动化测试覆盖迁移后重启恢复新目录、Tag 保留，以及定位文件写入失败时旧目录恢复；浏览器验收确认确认框、目录选择器、命令路径和成功状态正确。
- 当前测试：51 项通过。

### M7 - 验收与发布

- 使用仓库内 Tauri CLI 生成嵌入生产前端资源的 debug 应用，确认不依赖 Vite 开发服务器即可启动。
- 使用 D 盘临时受管目录完成真实 Windows 桌面流程：首次设置、native 目录选择、native Excel 选择、导入 5 行及 5 张嵌入图片。
- 在真实应用中添加 `CaseTag` 与 `caseTag`，确认按大小写分别计数、同一行同时显示，并在应用重启后完整恢复。
- 点击缩略图验证 2048px 图片预览弹窗可正常打开和关闭。
- 真实桌面验收发现 WebView `window.confirm` 无法调用；已统一改用 Tauri dialog 插件确认框，覆盖替换工作簿、迁移目录和批量删除 Tag。
- 已验证迁移确认框和批量删除确认框可正常显示；批量删除选择取消后没有修改 Tag。
- 已将受管目录完整迁移到新位置，确认数据库、工作簿和缓存存在，旧目录被清理；迁移后再次重启仍恢复 5 行及两个大小写 Tag。
- 已通过 native 保存对话框导出 5 行工作簿；检查 OOXML 确认最后一列为 `Tags`，H2 为 `CaseTag, caseTag`，原 drawing 关系仍保留。
- 前端生产构建和 51 项 Rust 自动化测试在确认框修复后全部通过。
- 使用仓库内 Tauri CLI 完成 Windows x64 release 优化构建和 NSIS 安装包生成，bundler 缓存及临时目录均定向到 `D:\Agent\Agent_temp`。
- 安装器 `智能表格_0.1.0_x64-setup.exe` 大小为 3,745,226 字节，SHA-256 为 `F09F3F680D1C995EEF7EBD665E8986F4D19B2A4DA9CB2736E36DB22A05DE5D61`。
- 已将安装包静默安装到 D 盘隔离测试目录，确认已安装应用可启动并恢复工作区；静默卸载返回成功并清理安装目录。
- `PLAN.md` 中全部 11 项验收标准均已通过。
- 已创建并推送首个版本 tag `v_1`，GitHub Release 已发布为 Latest。
- Release 地址：`https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_1`；已上传 `Smart-Spreadsheet_0.1.0_x64-setup.exe`。
- Windows 已知目录 API 不采用临时 APPDATA 环境变量；验收结束后已将测试定位文件备份到 D 盘，并清理本应用在 C 盘生成的状态目录和 WebView2 缓存。

### M8 - 首轮交互优化

- release 可执行文件声明为 Windows GUI 子系统，启动时不再附带空白命令行窗口；实际构建产物 PE Header 已验证 `Subsystem=2`。
- 默认窗口由 1440×900 缩小为 1180×760，最小窗口由 1080×680 调整为 900×600，继续支持用户自由缩放。
- 主表重构为 `行 / 图片 / Tags` 三列，时间、正负提示词、画师、图片文件夹、图片路径和嵌入图引用统一移入展开详情。
- 移除独立勾选列；点击行内任意非按钮区域可切换多选，Enter/Space 同样可选择，图片预览和展开详情不会误触选择。
- 表格取消横向滚动，并按实际纵向滚动条宽度动态收窄表头；浏览器测量确认三列表头与数据列的起点和宽度完全一致。
- 使用 10,000 行浏览器模拟数据复验虚拟化，首屏仍只渲染 14 行；连续点击与键盘操作可正确累计选择 1、2、3 行。
- 将 Tag 定义与行关联拆分：筛选区新增独立的“新建 Tag”入口，Tag 名称继续区分大小写；自定义 Tag 即使暂时没有关联任何行也会持久保留。
- 右键菜单改为搜索并复选 Tag 库中的已有 Tag，打开时回显该行当前 Tag，保存时在单个 SQLite 事务中原子完成新增和移除；操作只作用于被右键行，不读取或修改当前多选。
- 批量编辑取消每次手写 Tag，改为从 Tag 库点选一个或多个已有 Tag 后执行批量添加或删除。
- 右键菜单继续支持 Escape、点击外部、表格滚动、筛选重载或组件销毁时关闭；搜索框只筛选已有 Tag，不会隐式创建新 Tag。
- 浏览器使用 10,000 行模拟数据验收：先新建零关联的“苹果”和“香蕉”，右键第三行勾选两项后只更新该行；批量点选“苹果”可添加到另一行；移除最后一个“香蕉”关联后 Tag 库仍显示“香蕉 0”。
- 前端生产构建通过；Rust 共 53 项自动化测试通过，新增测试覆盖零关联 Tag、单行原子设置及未知 Tag 整体回滚。
- 开发服务器端口由 `1420` 调整为当前未占用的 `127.0.0.1:1422`，Vite 启动参数与 Tauri `devUrl` 保持一致；正式构建不依赖该端口。

### M9 - 前端重构（Svelte 工作台）

- 引入 Svelte 5（runes）+ `@sveltejs/vite-plugin-svelte` 7（兼容 Vite 8）+ `svelte-check`，新增 `vite.config.ts` 和 `svelte.config.js`，构建脚本改为 `svelte-check && vite build`；Node 版本保持不变。
- 整体替换手写 DOM 同步实现：删除旧 `tag-workspace.ts`、`virtual-table.ts`、`styles.css`，状态集中到 app / row / selection / tag 四个响应式 store。
- 新布局为浅色主题三栏工作台：顶栏（工作簿信息、画廊/表格切换、迁移/替换/导出）、左侧 Tag 库侧栏、中间数据视图、右侧常驻详情面板（可收起）。
- 画廊为默认视图：虚拟网格按视口宽度自适应列数，沿用 200 行/页分页缓存与缩略图队列；工作簿替换时清空缩略图缓存避免行 ID 复用串图。
- 表格视图为多列虚拟列表（勾选/缩略图/行号/时间/正向提示词/画师串/Tags），与画廊共享查询、分页缓存和选择状态。
- 详情面板取代“展开详情”弹窗和右键 Tag 菜单：单击行或卡片即显示大图（2048px 预览，缩略图先行占位）、全部字段（可复制）和该行 Tag 编辑器；大图可点击放大，单行 Tag 修改原位更新缓存不丢滚动位置。
- 选择交互改为勾选框 + Shift 范围选 + Ctrl+A 全选筛选结果；批量操作移入底部浮动操作条（选中时出现），从 Tag 库点选后批量添加/移除，沿用 explicit / filtered 双选择模型。
- Tag 库侧栏合并筛选与新建：点击 Tag 即筛选，AND / OR 与匹配数在侧栏顶部，新建入口在侧栏底部；筛选中但已被清理的 Tag 以计数 0 显示。
- 首次设置、导入、启动错误页简化为居中卡片；操作结果改为底部 toast 通知，成功提示 5 秒自动消失。
- 实机验收发现并修复 `$effect` 内调用含“读-改-写”逻辑导致的 `effect_update_depth_exceeded` 无限循环（用 `untrack` 包裹副作用调用）。
- Tauri 实机冒烟验证通过：导入样表后画廊、Tag 筛选、详情面板、字段复制和缩略图加载均正常；`svelte-check` 0 错误 0 警告。
- 窗口改为无系统边框（`decorations: false`）+ 内联标题栏：顶栏承担拖拽（`data-tauri-drag-region`，双击可最大化/还原），右上角自绘最小化/最大化/关闭按钮；流程页使用独立标题条。新增窗口控制相关 capability 权限。用户实机确认拖拽和窗口按钮可用。
- 应用版本提升到 0.2.0；`tauri.conf.json` 补充 bundle 图标配置并将打包目标固定为 NSIS。
- 安装器 `智能表格_0.2.0_x64-setup.exe` 大小为 3,774,303 字节，SHA-256 为 `F305CBFC7FF7090D17D7C785B2A1FBAA897FFA13B1D41386DD894B2FDA9833B4`；以 ASCII 文件名上传至 GitHub Release `v_2`。

### Release v_17 - 2026-07-03

- 使用仓库内 Tauri CLI 完成 Windows x64 release 构建，生成 NSIS 安装包 `智能表格_0.9.2_x64-setup.exe`。
- 发布前验证：`svelte-check` 0 错误、5 个既有可访问性警告；`vite build` 通过；`cargo test -- --test-threads=1` 通过 165 项、忽略 1 项，两个 Rust 集成测试均通过。
- 安装包大小为 4,383,854 字节，SHA-256 为 `FED15926B41836FF63E325FBA4BCB9B75C4539036BE45D740A70B70A9C4C0938`。
- 已以 ASCII 文件名 `Smart-Spreadsheet_0.9.2_x64-setup.exe` 上传至 GitHub Release `v_17`：`https://github.com/Achilng/Smart-Spreadsheet/releases/tag/v_17`。

## 建议用户实机抽查（自动化无法覆盖的 native 对话框链路）

- 文件夹/压缩包导入：选择对话框 → 进度横幅 → 完成提示（导入逻辑与进度事件已有单测覆盖）。
- 三种导出与智绘姬 JSON 去重：保存/选目录对话框 → 进度 → 结果提示；导出的 JSON 在智绘姬中实际加载一次。
- 元数据失败行目前在详情中带标记展示，但暂无专门筛选入口（候选后续迭代）。

## 风险与约束

- 上万行数据要求后端分页、虚拟列表和图片懒加载从第一版开始纳入设计。
- 导出必须基于内部工作簿副本，并验证除预期工作表 XML 外的 XLSX 内容未被破坏。
- 不安装全局 `cargo-tauri`；使用仓库内 `@tauri-apps/cli`，Node 版本保持不变。
