# 轮询脚本：图片集轮询（智能表格 JSON 独立导入）

改动全部在 `D:\Agent\Novelai轮询脚本`（注意：该目录**不是 git 仓库**，不做 git 操作）。智能表格的导出 JSON 格式完全不动。

## 核心语义
- 智能表格 JSON 从「批量填入」中移出，成为独立入口；选文件后弹窗勾选要导入的提示词字段（正向/角色/负面，默认全选）。
- 图片集启用时：**每张图 = 一个组合**，逐张按序生成，图的三段提示词原样写入 NovelAI 对应输入框；轮询区域不参与组合；固定区域仍按写入位置追加（图片提示词在前、固定区域在后，`",\n"` 连接）。
- 未勾选的字段 = 该输入框保持不动（正向提示词原本无条件写入，需新增开关，仅图片集模式下可能为 false）。

## 1. 数据模型与持久化
- `defaultConfig()` 新增 `imageSet: defaultImageSet()` → `{ enabled: false, fields: { positive: true, character: true, negative: true }, items: [] }`；item 结构 `{ id, name, prompts: { positive, character, negative } }`。
- 新增 `normalizeImageSet(raw)`（仿 `normalizeVibeRotation` 458-523 的防御式重建），在 `normalizeConfig`（525-558）的返回对象中加一行。旧存档/配置导入自动补默认值，`SCHEMA_VERSION` 保持 1 不动。
- `validateConfig` 新增规则：启用但无图片 → 报错；启用时不能与「指定提示词的 VIBE 组」同时用（互相矛盾）；启用时跳过「轮询区域至少一个项目/项目非空/全局非空」这些检查（轮询区域此时是摆设）。

## 2. 纯逻辑函数（可测，进 TEST_API）
- `parseSmartSpreadsheetRotationPayload(text)`：从现 `validateSmartSpreadsheetRotationPayload`（280-304）抽出格式/schema/结构校验（三字段必须是字符串），不再按 target 抽取。
- `inspectImageSetImport(text)`：解析为 `{ items, skipped, total }`，丢弃三段全空的项目，名称回退 `图片 NNN`，保留内部换行。
- `inspectBulkLoopImport`（306-318）改为：检测到智能表格 JSON 直接抛错提示「请使用图片集轮询的导入 JSON 入口」，只支持多行纯文本；删除旧的按 target 读取逻辑（`validateSmartSpreadsheetRotationPayload` 移除，测试同步重写）。
- `buildImageSetPromptCombinations(config)`：每张图一个组合（勾选字段才参与，空串跳过；固定区域追加在后；`selectedItems = [{ regionId: "image-set", itemId, name, ... }]`，下载文件名自然变成 `图片名_001.png`）。
- 从 `buildCombinations`（877-890）抽出 `composeWithVibeUnits(promptCombinations, vibeUnits)` 供两条路径复用（Vibe 单位仍与组合相乘，行为不变）。
- `configUsesCharacterPrompt`/`configUsesNegativePrompt`（892-904）加 `|| 图片集启用且勾选对应字段`；新增 `configUsesPositivePrompt`（非图片集模式恒 true 保持旧行为；图片集模式 = 勾选正向 或 存在正向固定区域）。
- `summarizeConfig`（906-928）：图片集模式下组合数 = 图片数 × Vibe 单位数，输出补充 `imageSetCount`。

## 3. 任务与引擎
- `createTask`（962-990）：按 `imageSet.enabled` 分派用 `buildImageSetPromptCombinations` 还是原 `buildCombinations`，快照/进度/暂停恢复机制不变。
- 引擎写提示词处（2292-2303）：新增 `managesPositivePrompt` 开关，为 false 时跳过 `setPrompt` 与校验（角色/负面维持现有开关逻辑）。`startTask` 预检（2507-2508）随 flag 函数自动生效。

## 4. UI
- 「提示词区域」区块后新增「图片集轮询」section：启用开关、`导入 JSON` 按钮（隐藏 `#image-set-file` input，仿配置导入 3355/3530 模式）、`清空` 按钮（confirm）；图片列表每行显示名称（可编辑）、非空字段徽标、上移/下移/删除；三个字段勾选框可事后改（禁止全部取消）；启用时提示「任务将逐张复现图片提示词，轮询区域不参与组合，固定区域仍会追加」。
- 新增 `renderImageSetImportDialog()`（仿 `renderBulkImportDialog` 2770-2793，挂到 2984 弹窗区）：显示文件名、张数/跳过数、三个字段复选框（默认全选）、替换/追加模式、「确认导入/取消」。应用时按当前勾选再过滤一次全空图（日志报告跳过数），写入 config 并自动启用，`persistNow + log + render`。
- 批量填入弹窗瘦身：删除 JSON 文件选择区（2781-2785）、`#bulk-import-source` 及对应 change 处理（3484-3503），文案改为仅支持多行纯文本。
- 模块级状态 `imageSetImportState`（仿 `bulkImportState`）+ 对应 handleAction 分支与 bindUiEvents 绑定。

## 5. 测试（tests/prompt-rotation.test.cjs，node:test 风格）
- 重写 L107/L140 旧 JSON 导入用例 → 批量填入遇 JSON 报错；`inspectImageSetImport` 校验/跳过/换行保留。
- 新增：图片集组合逐张生成、未勾选字段不参与、固定区域追加顺序、轮询区域被忽略、文件名取图片名；`configUsesPositivePrompt`；validateConfig 三条新规则；normalizeConfig 默认值；summarizeConfig 计数；Vibe × 图片集组合顺序。
- 跑 `npm test` 与 `npm run check`（Node 内置 test runner，无第三方依赖）。

## 6. 文档与版本
- userscript `@version` 1.8.0 → 1.9.0。
- DESIGN.md：按惯例新增「29 图片集轮询 v1.9」章节；更新 3.1 中第 93 行的批量填入描述、第 7 章交叉组合规则加图片集例外。
- README.md：已实现功能第 21 行要点同步更新。

## 7. 收尾（可选）
- 智能表格仓库：导出菜单 hint 文案微调为「导出后可在轮询脚本逐张复现」并按 AGENTS.md 提交推送 GitHub（仅文案，一行改动）。