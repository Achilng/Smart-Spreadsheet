//! Tool schemas exposed to the model and their dispatch onto the library database.
//!
//! Core tools are always offered. The rest sit in groups the model loads on demand with
//! `load_tools`; a loaded group stays available for the rest of the conversation.
use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use super::{Engine, library_data, sql, vision};
use crate::app::AppRuntime;
use crate::automation::model::AutomationRuleDraft;
use crate::db::agent::{
    Action, COUNT_FIELDS, CountSort, DEFAULT_PROMPT_STATS_LIMIT, LibraryOp, MAX_PROMPT_STATS_LIMIT, PLACEHOLDERS,
    PromptStatsSort, ROW_FIELDS, RowSort, RulePatch, Target,
};

pub struct ToolOutput {
    /// Sent back to the model.
    pub output: Value,
    /// Rendered by the agent window (cards, counts).
    pub display: Value,
}

pub struct ToolGroup {
    pub key: &'static str,
    pub label: &'static str,
    /// One line in the system prompt.
    pub summary: &'static str,
    /// Returned by `load_tools`: how to use the group well.
    pub guide: &'static str,
    pub tools: &'static [&'static str],
}

const CORE: &[&str] = &[
    "get_main_window_state", "get_library_overview", "find_rows", "count_rows", "summarize_rows", "analyze_text",
    "list_artist_strings", "read_library_item", "view_images", "preview_changes", "apply_changes", "undo_operation",
    "list_operations", "show_selected_images", "read_app_guide", "load_tools",
];

pub const GROUPS: &[ToolGroup] = &[
    ToolGroup {
        key: "mainWindow",
        label: "主窗口控制",
        summary: "勾选图片、设置搜索/TAG/去重等筛选、切换视图与排序、打开某个分组或某张图的详情、打开图片对比窗口",
        guide: "- 只要求勾选时用 set_main_selection；要设置搜索框、TAG、画师串、去重、只看单画师、只看有 VIBE 等筛选控件时用 set_main_filters。\n- control_main_window 可切换视图（画廊/表格/分组/重复/素材/提示词）、排序、重复视图的去重方式，打开某个分组、在详情面板显示某张图、清空筛选并在画廊定位某张图，或打开图片对比窗口（A 必填、B 可选）。\n- 这些操作只改界面不改数据，不需要预演；发出后可再调用 get_main_window_state 确认结果。定位图片会清空用户当前的筛选，用之前先说一声。",
        tools: &["set_main_selection", "set_main_filters", "control_main_window"],
    },
    ToolGroup {
        key: "library",
        label: "分组 / TAG / 画师串管理",
        summary: "重命名、合并、删除分组和 TAG，清理空分组，设置画师串别名和代表图，查找可补 artist: 前缀的画师名",
        guide: "- 所有操作都先用 preview_library_change 预演（不改数据），把预演卡片里的影响范围讲给用户，用户同意后再 apply_changes(previewId)。每次都能撤销。\n- operation.type：renameGroup{group,newName}；mergeGroups{groups:[...],into}（into 可以是新名字，会自动新建；合并后删掉来源分组）；deleteGroups{groups:[...]}（图片只是变成未分组）；deleteEmptyGroups{}；renameTag{tag,newName}（新名字已存在时改用 mergeTags）；mergeTags{tags:[...],into}；deleteTags{tags:[...]}（素材上的同名 TAG 也会一起处理）；setArtistAlias{artistString,alias}（alias 为空表示清除）；setArtistRepresentative{rowId,enabled}。group 可以写分组 id（数字）或分组名。TAG 名区分大小写。\n- 删除和合并影响面大：预演前先用 get_library_overview / count_rows 看清楚张数；删除分组或 TAG 不会删除图片，要这样告诉用户。\n- list_artist_prefix_candidates 列出「在别的图里写过 artist:名字、但有些图里只写了裸名字」的画师名，可据此用 preview_changes 的 prefixArtist 动作补前缀。",
        tools: &["preview_library_change", "list_artist_prefix_candidates"],
    },
    ToolGroup {
        key: "automation",
        label: "自动规则",
        summary: "新建、修改、启用/停用、删除自动规则，以及把某条规则在全库试跑一遍",
        guide: "- 规则的读取：list_library_items(collection=automationRules) 列出，read_library_item(kind=automationRule) 看详情；找出规则会命中哪些图可在任何 target 里写 ruleId。\n- create_automation_rule / update_automation_rule / delete_automation_rule 会直接生效（可在操作记录里撤销），所以只有用户明确同意后才能调用；改之前先把要改成什么讲清楚。update 只传要改的字段，例如停用规则：patch={enabled:false}。\n- 条件格式：{mode:'all'|'any', negate?, groups:[{mode, conditions:[...]}]}，单个条件写法同 target.conditions。动作写法同 preview_changes，但 setGroup 用 {groupId}（必须是已有分组），没有 setFavorite，多一个 stopProcessing{}（命中后不再执行后面的规则）。\n- preview_rule_on_library 把规则的条件和动作在全库（或指定 target）上预演一次，得到 previewId，用户同意后 apply_changes 执行，可撤销。它不会改规则本身。",
        tools: &["create_automation_rule", "update_automation_rule", "delete_automation_rule", "preview_rule_on_library"],
    },
    ToolGroup {
        key: "records",
        label: "素材 / 提示词文档 / 导入批次",
        summary: "分页列出素材、提示词文档、自动规则和导入批次",
        guide: "- list_library_items 分页列出 materials / promptDocs / automationRules / importBatches，拿到 ID 后用 read_library_item 读详情（素材版本、文档正文、规则内容）。\n- 导入批次的 id 可以直接用在 target.batchIds 里筛出这次导入的图；只想看最近导入的图也可以用 target.importedAfter。",
        tools: &["list_library_items"],
    },
    ToolGroup {
        key: "attachments",
        label: "聊天附件",
        summary: "读取用户上传图片的生成元数据、查看上传图片的画面（对话里有图片附件时会自动加载）",
        guide: "- read_attachment_metadata 读上传图片里的 NovelAI 生成信息；view_attachment_image 看画面，可要高清或按原图坐标裁切。附件的 messageId / attachmentNumber 不是图库 rowId。",
        tools: &["read_attachment_metadata", "view_attachment_image"],
    },
    ToolGroup {
        key: "sql",
        label: "只读 SQL 查询",
        summary: "现有工具查不了的组合条件、按命中几个 TAG 筛图、跨表统计时，直接写只读 SQL 查图库；查到的图可用 target.sql 交给预演等工具",
        guide: super::sql::GUIDE,
        tools: &["query_library_sql"],
    },
];

/// Tools kept only so that older conversations that called them still replay.
const LEGACY: &[&str] = &["list_generation_values"];

pub fn group_of(name: &str) -> Option<&'static ToolGroup> {
    GROUPS.iter().find(|group| group.tools.contains(&name))
}

/// The system-prompt paragraph that tells the model which groups it can load.
pub fn groups_prompt() -> String {
    GROUPS
        .iter()
        .map(|group| format!("- {}（{}）：{}", group.key, group.label, group.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tool names to offer: the core set, everything the conversation loaded or used, and `extra`.
pub fn definitions(active: &HashSet<String>) -> Value {
    let names = CORE.iter().map(|name| (*name).to_owned()).chain(active.iter().cloned()).collect::<HashSet<_>>();
    Value::Array(all_definitions().into_iter().filter(|tool| tool["name"].as_str().is_some_and(|name| names.contains(name))).collect())
}

fn target_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": {
            "scope": {"type": "string", "enum": ["selection", "filteredView", "all", "rowIds"],
                "description": "selection=主窗口当前选中；filteredView=主窗口当前筛选结果；all=全库；rowIds=指定 id"},
            "rowIds": {"type": "array", "items": {"type": "integer"}},
            "artistStrings": {"type": "array", "items": {"type": "string"},
                "description": "只保留属于这些画师串分组的图（按画师串去重的分组键匹配，写法差异会被忽略）"},
            "expandToSameArtistString": {"type": "boolean",
                "description": "先把范围扩展到与范围内图片同画师串的全库所有图"},
            "expandToSame": {"type": "array", "items": {"type": "string", "enum": ["artists", "positivePrompt", "vibes"]},
                "description": "把范围扩展到全库里画师串 / 正向提示词（去掉首尾空白后完全相同）/ VIBE 组合与范围内图片相同的图，和主窗口「重复」视图的分组一致"},
            "favorite": {"type": "boolean", "description": "只保留收藏/未收藏的图"},
            "modelBadge": {"type":"string","description":"精确匹配图库卡片上的模型徽章，如 V4.5F、5F、v4 C；自动识别已知构建哈希。"},
            "batchIds": {"type":"array","items":{"type":"integer"},"description":"只保留这些导入批次的图（批次 id 见 count_rows by=batch 或 list_library_items importBatches）"},
            "importedAfter": {"type":"string","description":"导入时间不早于此（本地时间，如 2026-11-20 或 2026-11-20 14:30）"},
            "importedBefore": {"type":"string","description":"导入时间早于此（不含）"},
            "updatedAfter": {"type":"string","description":"最后修改时间不早于此（改提示词、备注、分组、TAG 都算修改）"},
            "updatedBefore": {"type":"string","description":"最后修改时间早于此（不含）"},
            "ruleId": {"type":"integer","description":"只保留会被这条自动规则的条件命中的图"},
            "conditions": {"type": "array", "items": {"type": "object"},
                "description": "附加自动规则条件，格式见系统提示中的条件格式。按实际记录的采样器名筛选示例：{type:'generationText',field:'sampler',operator:'equals',value:'k_euler_ancestral'}。"},
            "conditionMatch": {"type": "string", "enum": ["all", "any"], "description": "多个条件是全部满足还是任一满足，默认 all"},
            "sql": {"type": "string", "description": "只读 SQL（先 load_tools 加载 sql 工具组看表结构），第一列是图片 id。scope=all 时范围就是查出来的图，scope=rowIds 时和 rowIds 取交集；其它字段照常叠加"}
        },
        "required": ["scope"],
        "additionalProperties": false
    })
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": {"type": "object", "properties": properties, "required": required},
    })
}

fn all_definitions() -> Vec<Value> {
    let placeholders = PLACEHOLDERS.iter().map(|name| format!("{{{name}}}")).collect::<Vec<_>>().join(" ");
    let actions = json!({
        "type": "array",
        "description": format!("按顺序执行的修改动作。每个动作是一个对象，type 取值：\
addTags{{tags:[...]}}；removeTags{{tags:[...]}}；setGroup{{groupName, onlyIfUngrouped?}}（分组不存在会新建）；clearGroup{{}}；\
appendPrompt{{field,value}}（在末尾追加 TAG）；deletePromptTags{{field,value}}（删除逗号分隔的整个 TAG，value 可写多个，用逗号分隔；忽略大小写和权重写法，masterpiece 也会删掉 {{masterpiece}}、1.2::masterpiece::；其余内容、分隔符和换行原样保留；跨多个 TAG 的权重组如 2::masterpiece, best quality:: 只删这个 TAG，保留 2:: 和 ::）；\
replacePrompt{{field,find,replace,caseSensitive}}（普通文本替换，替换所有出现）；\
regexReplacePrompt{{field,pattern,replace,caseSensitive}}（正则替换所有匹配，Rust regex 语法，replace 可用 $1）；\
setNote{{value}}；appendNote{{value,separator?}}；clearNote{{}}；setNoteSequence{{prefix}}；setFavorite{{favorite}}；\
prefixArtist{{artists:[...]}}（把三个提示词字段里这些画师名的裸写法补成 artist:名字）。\
field 取 positive / character / negative。caseSensitive 必须明确给出。改正向和角色要分别写两个动作。\
按图片代入：setGroup.groupName、setNote.value、appendNote.value、addTags.tags 里可写占位符 {placeholders}，每张图代入自己的值，\
例如 setGroup{{groupName:'{{artistAlias}}'}} 按画师串各自分组（有别名用别名），addTags{{tags:['{{modelBadge}}']}} 按模型打 TAG；缺少对应信息的图这一步不生效，预演 warnings 会给出数量。"),
        "items": {"type": "object", "properties": {"type": {"type": "string"}}, "required": ["type"]}
    });
    let fields = json!({"type": "array", "items": {"type": "string", "enum": ROW_FIELDS}});
    vec![
        tool("load_tools", "加载一组额外工具（系统提示里列出的工具组）。加载后从下一步起可用，并返回这组工具的用法说明；本对话里一直有效。", json!({
            "groups": {"type":"array","items":{"type":"string","enum": GROUPS.iter().map(|group| group.key).collect::<Vec<_>>()}}
        }), &["groups"]),
        tool("read_app_guide", "查看智能表格的功能说明：某个功能在软件里的入口和用法（导入、导出、删除图片、筛选、分组、TAG、对比、素材、自动规则、模型设置等）。用户问「怎么做 / 在哪里」或者你做不到需要告诉用户去哪手动操作时先查这里，不要凭印象回答。不带 topic 返回总览和主题目录。", json!({
            "topic": {"type":"string","description":"主题关键词，如 导出、删除、TAG、对比"}
        }), &[]),
        tool("read_attachment_metadata", "在本地读取当前对话一张上传图片的生成元数据（不是看图，也不是图库 rowId）。使用附件旁的 messageId 和 attachmentNumber。PNG 尝试读取 NovelAI 信息，JPEG/WebP 仅基本信息。", json!({
            "messageId": {"type":"integer", "minimum":1},
            "attachmentNumber": {"type":"integer", "minimum":1, "maximum":4, "description":"消息中从 1 开始的附件序号，包含非图片附件"}
        }), &["messageId", "attachmentNumber"]),
        tool("view_attachment_image", "按需查看当前对话的一张上传图片（不是图库 rowId）。默认长边最多 2000px，high 最多 3072px；大图可能进一步缩小。crop 使用未旋转原图像素，左上角为 (0,0)，不是预览坐标。结果之后只附一张视图。", json!({
            "messageId": {"type":"integer", "minimum":1},
            "attachmentNumber": {"type":"integer", "minimum":1, "maximum":4},
            "detail": {"type":"string", "enum":["standard","high"]},
            "crop": {"type":"object", "properties": {
                "x":{"type":"integer","minimum":0}, "y":{"type":"integer","minimum":0},
                "width":{"type":"integer","minimum":1}, "height":{"type":"integer","minimum":1}
            }, "required":["x","y","width","height"], "additionalProperties":false}
        }), &["messageId", "attachmentNumber"]),
        tool("get_library_overview", "图库概况（总张数、收藏、画师串分组、缺少元数据、AI 提取画师串的张数、导入批次数和最近导入时间、自动规则数、空分组数），以及分组和 TAG 列表。分组和 TAG 各自分页，同一 offset 和 limit。", json!({
            "offset":{"type":"integer","minimum":0}, "limit":{"type":"integer","minimum":1,"maximum":100}
        }), &[]),
        tool("list_library_items", "分页列出素材、提示词文档、自动规则或导入批次。用返回的 ID 查详情。", json!({
            "collection":{"type":"string","enum":["materials","promptDocs","automationRules","importBatches"]},
            "offset":{"type":"integer","minimum":0}
        }), &["collection"]),
        tool("read_library_item", "按 ID 读取图库记录、PNG 原图元数据（含逐角色正负词）、素材版本、提示词文档或自动规则。条目不大时（如自动规则）一次返回全部内容（complete=true），不必再逐层展开；内容很大时只给出大字段的摘要，再按 JSON Pointer 路径展开，例如 /positivePrompt、/chunks/Comment/sampler、/characters/0/negativePrompt、/versions/0/text。按 nextOffset 续读。", json!({
            "kind":{"type":"string","enum":["row","imageMetadata","material","promptDoc","automationRule"]},
            "id":{"type":"string","description":"条目 ID，数字编号也可写成字符串"},
            "path":{"type":"string"}, "offset":{"type":"integer","minimum":0}
        }), &["kind","id"]),
        tool("get_main_window_state", "读取主窗口当前状态：详情面板里的图片 ID、勾选数、筛选条件和结果数、视图模式、排序、分组视图里展开的分组、重复视图里展开的重复组（含一张样例图 ID，可配合 target.expandToSame 取整组），以及图片对比窗口里的 A/B 图。用户说「这张」「选中的」「现在这些」「这个分组」「这两张」时先调用。", json!({}), &[]),
        tool("list_generation_values", "（旧工具，请改用 count_rows）列出范围内实际记录的模型原名、模型徽章、采样器或噪声调度的张数。", json!({
            "target":target_schema("要统计的范围"),
            "field":{"type":"string","enum":["model","modelBadge","sampler","noiseSchedule"]},
            "offset":{"type":"integer","minimum":0}
        }), &["target","field"]),
        tool("count_rows", "按 1~2 个字段统计范围内各取值的图片张数（两个字段时是交叉表），每项带占比和最多 3 张样例 ID。可统计模型、徽章、采样器、步数、CFG、尺寸、横竖、VIBE 数量、收藏、分组、TAG、导入批次、导入日期、最后修改日期、画师串、文件夹、是否有备注/角色提示词等。用它直接聚合，不要用 find_rows 翻页数数。", json!({
            "target": target_schema("统计范围"),
            "by": {"type":"array","items":{"type":"string","enum": COUNT_FIELDS},"minItems":1,"maxItems":2},
            "sortBy": {"type":"string","enum":["count","value"],"description":"count=按张数降序（默认）；value=按取值排序，适合看日期或数值分布"},
            "limit": {"type":"integer","minimum":1,"maximum":100}, "offset": {"type":"integer","minimum":0}
        }), &["target","by"]),
        tool("find_rows", "按范围读取图库图片的模型原名、徽章、采样器、生成参数、尺寸、提示词、整理信息、导入/修改时间和文件来源。每页最多 50 张。长文字显示摘要，可用 read_library_item 续读。", json!({
            "target": target_schema("要查看的图片范围"),
            "fields": fields,
            "sort": {"type":"string","enum":["oldest","newest","recentlyUpdated"],"description":"oldest=按导入先后（默认），newest=最新导入在前，recentlyUpdated=最近修改在前"},
            "limit": {"type": "integer"},
            "offset": {"type": "integer"}
        }), &["target"]),
        tool("view_images", "真正查看图库图片画面（需要模型支持图片输入）。一次最多 16 张，按 rowIds 顺序展示。1~2 张时逐张给你画廊预览（长边约 1024）；3 张及以上拼成一张总览图，每格下方标着 #图片编号，单格分辨率较低。只看 1 张时可以加 detail=high 读原图高清（长边最多 3072），或用 crop 按原图像素裁切局部（左上角为 0,0，原图尺寸见 find_rows 的 width/height），用来看清小字、手、水印等细节。图片会在工具结果之后作为一条单独的消息附上。", json!({
            "rowIds": {"type": "array", "items": {"type": "integer"}, "description": "要查看的图片 id，最多 16 个"},
            "detail": {"type":"string","enum":["standard","high"],"description":"只在 rowIds 只有 1 张时可用：读原图，standard 长边最多 2000，high 最多 3072"},
            "crop": {"type":"object","description":"只在 rowIds 只有 1 张时可用：原图像素坐标的裁切框","properties": {
                "x":{"type":"integer","minimum":0}, "y":{"type":"integer","minimum":0},
                "width":{"type":"integer","minimum":1}, "height":{"type":"integer","minimum":1}
            }, "required":["x","y","width","height"], "additionalProperties":false}
        }), &["rowIds"]),
        tool("analyze_text", "统计一段文字或正则在范围内各字段的真实命中情况：命中多少张、出现几次、不同写法、出现在开头/中间/结尾、一张图多次命中。改提示词前必须先用它看边界情况。", json!({
            "target": target_schema("统计范围"),
            "pattern": {"type": "string"},
            "isRegex": {"type": "boolean"},
            "caseSensitive": {"type": "boolean"},
            "fields": {"type": "array", "items": {"type": "string", "enum": ["positive", "character", "negative", "artists", "note"]},
                "description": "默认 positive、character、negative"}
        }), &["target", "pattern", "isRegex", "caseSensitive"]),
        tool("summarize_rows", "统计范围内最常见的正面提示词、角色提示词、负面提示词和独立画师项。每榜独立，同一项在同一张图出现多次只计一次；返回范围内张数、占比（0~1）和最多 3 张样例图片 ID。用它直接聚合，不要用 find_rows 翻页做统计。", json!({
            "target": target_schema("要统计的图片范围"),
            "lists": {"type":"array","items":{"type":"string","enum":["positive","character","negative","artists"]},"description":"要哪些榜单，默认 positive、character、artists"},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_PROMPT_STATS_LIMIT, "default": DEFAULT_PROMPT_STATS_LIMIT, "description": "每个榜单最多返回多少项"},
            "compareLibrary": {"type": "boolean", "default": false, "description": "是否同时返回全库占比及富集倍数（范围内占比除以全库占比）"},
            "sortBy": {"type": "string", "enum": ["count", "enrichment"], "default": "count", "description": "按范围内张数或富集倍数降序；enrichment 会自动开启全库对比"}
        }), &["target"]),
        tool("list_artist_strings", "列出范围内的画师串分组（与主窗口画师串去重视角一致），可按文字或别名搜索。返回每组在范围内和全库的图片数、别名和代表图。", json!({
            "target": target_schema("统计范围，一般用 all"),
            "contains": {"type": "string", "description": "画师串或别名包含的文字（不区分大小写）"},
            "limit": {"type": "integer"}, "offset":{"type":"integer","minimum":0}
        }), &["target"]),
        tool("preview_changes", "预演一次修改（不会改数据）。返回 previewId、会改多少张、每个字段改多少、匹配分析、边界提醒和修改前后样例。用户会在界面上看到这份预演。", json!({
            "title": {"type": "string", "description": "一句话说明这次修改，例如「删除正向里 Text: 及之后的内容」"},
            "target": target_schema("要修改的图片范围"),
            "actions": actions
        }), &["title", "target", "actions"]),
        tool("apply_changes", "执行一个预演（preview_changes、preview_library_change 或 preview_rule_on_library 得到的 previewId）。只有需求完全明确、预演结果符合用户预期时才能调用。", json!({
            "previewId": {"type": "string"}
        }), &["previewId"]),
        tool("undo_operation", "撤销一次已执行的修改（包括图库整理和自动规则的增删改）。之后又被改过的图会保留现状并计入跳过数。", json!({
            "operationId": {"type": "integer"}
        }), &["operationId"]),
        tool("list_operations", "列出最近执行过的修改记录（含 id、类型、状态）。", json!({
            "limit": {"type": "integer"}
        }), &[]),
        tool("set_main_selection", "在主窗口中选中范围内的所有图片（不改数据）。", json!({
            "target": target_schema("要选中的图片")
        }), &["target"]),
        tool("show_selected_images", "在主窗口选中并仅显示目标图片。筛图给用户查看时优先调用，可直接传完整筛选条件；查看已勾选的图片用 scope=selection。", json!({
            "target": target_schema("要单独展示的图片范围")
        }), &["target"]),
        tool("set_main_filters", "设置主窗口的筛选控件（不改数据）。clear=true 先清空所有筛选（排序保留）。", json!({
            "clear": {"type": "boolean"},
            "search": {"type": "string", "description": "搜索框文字"},
            "tags": {"type": "array", "items": {"type": "string"}},
            "tagMode": {"type": "string", "enum": ["and", "or"]},
            "artistFilter": {"type": "string", "description": "只看这个画师串"},
            "dedupe": {"type": "string", "enum": ["none", "artists", "positivePrompt", "vibes"],
                "description": "去重视角：artists=画师串去重"},
            "singleArtistOnly": {"type": "boolean", "description": "只看单画师的图"},
            "hasVibe": {"type": "boolean", "description": "只看用了 VIBE 的图"},
            "untaggedOnly": {"type": "boolean"},
            "hideGrouped": {"type": "boolean"}
        }), &[]),
        tool("control_main_window", "操作主窗口界面（不改数据）：切换视图、排序、重复视图的去重方式，展开某个分组，在详情面板显示某张图，清空筛选并在画廊定位某张图，或打开图片对比窗口。可一次给多个。", json!({
            "viewMode": {"type":"string","enum":["gallery","table","group","duplicates","materials","promptDocs"],"description":"画廊/表格/分组/重复/素材/提示词"},
            "sort": {"type":"string","enum":["oldest","newest","recentlyUpdated"]},
            "duplicatesMode": {"type":"string","enum":["artists","positivePrompt","vibes"],"description":"重复视图按什么分组，会自动切到重复视图"},
            "openGroupId": {"type":"integer","description":"切到分组视图并展开这个分组"},
            "openDetailRowId": {"type":"integer","description":"在详情面板显示这张图（不改筛选）"},
            "locateRowId": {"type":"integer","description":"清空筛选，切到画廊并滚动到这张图"},
            "compare": {"type":"object","properties":{"rowId":{"type":"integer","description":"参考图 A"},"targetRowId":{"type":"integer","description":"可选的对比图 B"}},"required":["rowId"],"additionalProperties":false}
        }), &[]),
        tool("preview_library_change", "预演一次分组 / TAG / 画师串的整理（不会改数据），得到 previewId，用户同意后用 apply_changes 执行，可撤销。用法见 library 工具组说明。", json!({
            "title": {"type":"string","description":"一句话说明，例如「把 TAG 风景 合并到 landscape」"},
            "operation": {"type":"object","description":"{type: renameGroup|mergeGroups|deleteGroups|deleteEmptyGroups|renameTag|mergeTags|deleteTags|setArtistAlias|setArtistRepresentative, ...}","properties":{"type":{"type":"string"}},"required":["type"]}
        }), &["title","operation"]),
        tool("list_artist_prefix_candidates", "列出可以补 artist: 前缀的画师名：这些名字在别的图里明确写过 artist:名字，但有些图的提示词里只写了裸名字。返回每个名字涉及的图片数和样例。", json!({}), &[]),
        tool("create_automation_rule", "创建一条自动规则（以后导入新图时自动执行）。只有用户明确同意建立自动规则后才能调用。", json!({
            "rule": {"type": "object", "description": "自动规则：{name, description?, enabled?, runOnImport?(默认 true), runOnUpdate?, conditions:{mode:'all'|'any', groups:[{mode, conditions:[...]}]}, actions:[...]}。actions 格式同 preview_changes，但 setGroup 用 {groupId}（必须是已有分组 id），不支持 setFavorite。"}
        }), &["rule"]),
        tool("update_automation_rule", "修改一条自动规则（只改 patch 里给出的字段），也用于启用或停用。直接生效、可撤销；只有用户明确同意后才能调用。", json!({
            "ruleId": {"type":"integer"},
            "patch": {"type":"object","description":"{name?, description?, enabled?, runOnImport?, runOnUpdate?, conditions?, actions?}，conditions / actions 会整体替换","additionalProperties":false,"properties":{
                "name":{"type":"string"},"description":{"type":"string"},"enabled":{"type":"boolean"},
                "runOnImport":{"type":"boolean"},"runOnUpdate":{"type":"boolean"},
                "conditions":{"type":"object"},"actions":{"type":"array","items":{"type":"object"}}
            }}
        }), &["ruleId","patch"]),
        tool("delete_automation_rule", "删除一条自动规则。直接生效、可撤销；只有用户明确同意后才能调用。", json!({
            "ruleId": {"type":"integer"}
        }), &["ruleId"]),
        tool("preview_rule_on_library", "把一条已有自动规则的条件和动作在全库（或指定范围）上预演，得到 previewId；用户同意后 apply_changes 执行，可撤销。不会改规则本身。", json!({
            "ruleId": {"type":"integer"},
            "target": target_schema("在哪些图上试跑，默认全库")
        }), &["ruleId"]),
        tool("query_library_sql", "对图库执行一条只读 SQL（SQLite，只能 SELECT / WITH），返回列名和结果行。表结构、可用函数（has_tag、tag_hits、prompt_tags、REGEXP）和例子见 sql 工具组说明。默认返回 100 行，limit 最多 1000，并给出总行数。", json!({
            "sql": {"type":"string","description":"一条 SELECT 语句"},
            "limit": {"type":"integer","minimum":1,"maximum":1000,"description":"最多返回多少行，默认 100"}
        }), &["sql"]),
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetArgs {
    target: Target,
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    sort: RowSort,
}

fn parse<T: for<'de> Deserialize<'de>>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| format!("参数格式不对：{error}"))
}

fn changed(app: &AppHandle) {
    let _ = app.emit("agent://library-changed", ());
    let _ = app.emit("agent://operations-changed", ());
}

fn rules_changed(app: &AppHandle) {
    let _ = app.emit("automation-rules://changed", ());
    let _ = app.emit("agent://operations-changed", ());
}

/// The preview as the model sees it: groups keep a few example IDs instead of every row.
fn preview_for_model(report: &Value) -> Value {
    let mut value = report.clone();
    for group in value["changeGroups"].as_array_mut().into_iter().flatten() {
        if let Some(ids) = group["rowIds"].as_array_mut() {
            ids.truncate(5);
        }
    }
    value
}

fn preview_output(report: Value) -> ToolOutput {
    ToolOutput { output: preview_for_model(&report), display: json!({"kind": "preview", "report": report}) }
}

fn operation_output(record: &crate::db::agent::OperationRecord, output: Value) -> Result<ToolOutput, String> {
    let value = serde_json::to_value(record).map_err(|e| e.to_string())?;
    Ok(ToolOutput { output, display: json!({"kind": "operation", "operation": value}) })
}

const GUIDE: &str = include_str!("guide.md");

/// The overview and table of contents, or the sections about one topic.
fn read_guide(topic: Option<&str>) -> Value {
    let (intro, rest) = GUIDE.split_once("\n## ").map_or((GUIDE, ""), |(intro, rest)| (intro, rest));
    let sections = if rest.is_empty() { Vec::new() } else {
        rest.split("\n## ").map(|section| format!("## {}", section.trim_end())).collect::<Vec<_>>()
    };
    let heading = |section: &str| section.lines().next().unwrap_or_default().trim_start_matches("## ").to_owned();
    let topics = sections.iter().map(|section| heading(section)).collect::<Vec<_>>();
    let Some(topic) = topic.map(str::trim).filter(|topic| !topic.is_empty()) else {
        return json!({"overview": intro.trim(), "topics": topics, "note": "用 topic 读取某个主题的具体入口和步骤"});
    };
    let needle = topic.to_lowercase();
    let mut matched = sections.iter().filter(|section| heading(section).to_lowercase().contains(&needle)).cloned().collect::<Vec<_>>();
    if matched.is_empty() {
        matched = sections.iter().filter(|section| section.to_lowercase().contains(&needle)).cloned().collect();
    }
    if matched.is_empty() {
        return json!({"found": false, "topics": topics, "note": "没有找到这个主题，换一个关键词或从 topics 里选"});
    }
    let mut text = matched.into_iter().take(3).collect::<Vec<_>>().join("\n\n");
    if text.chars().count() > 6000 {
        text = text.chars().take(6000).collect::<String>() + "…（已截断，换更具体的关键词）";
    }
    json!({"found": true, "text": text})
}

pub fn run(app: &AppHandle, conversation_id: &str, call_id: &str, name: &str, arguments: &Value) -> Result<ToolOutput, String> {
    let runtime = app.state::<AppRuntime>();
    let engine = app.state::<Engine>();
    let view = engine.main_view();
    let expanded = sql::expand_target(&runtime, arguments)?;
    let arguments = expanded.as_ref().unwrap_or(arguments);
    let plain = |output: Value| Ok(ToolOutput { display: json!({"kind": "info"}), output });
    match name {
        "load_tools" => {
            #[derive(Deserialize)]
            struct Args { groups: Vec<String> }
            let args: Args = parse(arguments)?;
            let mut loaded = Vec::new();
            let mut guides = Vec::new();
            for key in &args.groups {
                let group = GROUPS.iter().find(|group| group.key == key).ok_or_else(|| format!("没有名为 {key} 的工具组"))?;
                loaded.push(group.key);
                let mut guide = json!({"group": group.key, "label": group.label, "tools": group.tools, "guide": group.guide});
                if group.key == "sql" {
                    guide["schema"] = sql::schema(&runtime)?;
                }
                guides.push(guide);
            }
            Ok(ToolOutput {
                output: json!({"loaded": guides, "note": "这些工具从下一步起可用"}),
                display: json!({"kind": "tools", "groups": loaded}),
            })
        }
        "read_app_guide" => plain(read_guide(arguments["topic"].as_str())),
        "get_library_overview" => {
            #[derive(Deserialize)]
            struct Page { #[serde(default)] offset: usize, limit: Option<usize> }
            let args: Page = parse(arguments)?;
            plain(runtime.agent_db(false, |db| db.agent_overview(args.offset, args.limit.unwrap_or(80).clamp(1,100)))?)
        }
        "list_library_items" => plain(library_data::list(&runtime, parse(arguments)?)?),
        "read_library_item" => plain(library_data::read(&runtime, parse(arguments)?)?),
        "query_library_sql" => {
            let output = sql::query(&runtime, parse(arguments)?)?;
            Ok(ToolOutput { display: json!({"kind": "sql", "totalRows": output["totalRows"]}), output })
        }
        "list_generation_values" => {
            #[derive(Deserialize)]
            struct Args { target: Target, field: String, #[serde(default)] offset: usize }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| db.agent_generation_values(&args.target, &view, &args.field, args.offset))?)
        }
        "count_rows" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args { target: Target, by: Vec<String>, #[serde(default)] sort_by: CountSort, limit: Option<usize>, #[serde(default)] offset: usize }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| db.agent_count_rows(&args.target, &view, &args.by, args.limit.unwrap_or(30), args.offset, args.sort_by))?)
        }
        "get_main_window_state" => {
            let info = engine.main_info();
            let counts = runtime.agent_db(false, |db| {
                let mut count = |selection: &Option<crate::db::RowSelection>| -> Result<Option<usize>, String> {
                    match selection {
                        Some(selection) => Ok(Some(db.selected_row_ids(selection).map_err(|e| e.to_string())?.len())),
                        None => Ok(None),
                    }
                };
                Ok((count(&view.selection)?, count(&view.filtered)?))
            })?;
            let compare = if app.get_webview_window("compare").is_some() { engine.compare_info() } else { Value::Null };
            if info.is_null() {
                return plain(json!({"reported": false, "message": "主窗口还没有上报状态（可能没打开资料库）", "compareWindow": compare}));
            }
            plain(json!({"selectedCount": counts.0.unwrap_or(0), "filteredCount": counts.1, "mainWindow": info, "compareWindow": compare}))
        }
        "view_attachment_image" => {
            let view: vision::AttachmentView = parse(arguments)?;
            let (url, output) = vision::render_attachment(&runtime, conversation_id, &view)?;
            engine.vision.put_attachment(conversation_id, call_id, &view, url);
            Ok(ToolOutput { display: json!({"kind":"info", "view":view}), output })
        }
        "read_attachment_metadata" => {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args { message_id: i64, attachment_number: u8 }
            let args: Args = parse(arguments)?;
            let (filename, encoded) = runtime.agent_db(false, |db| db.agent_image_attachment(conversation_id, args.message_id, args.attachment_number))?;
            let bytes = STANDARD.decode(encoded).map_err(|_| "无法读取已保存的图片附件")?;
            let metadata = crate::pipeline::attachment_metadata::inspect(&bytes)?;
            plain(json!({"fileName": filename, "metadata": metadata}))
        }
        "find_rows" => {
            let args: TargetArgs = parse(arguments)?;
            let output = runtime.agent_db(false, |db| {
                db.agent_find_rows(&args.target, &view, &args.fields, args.limit.unwrap_or(20), args.offset.unwrap_or(0), args.sort)
            })?;
            let ids = output["rows"].as_array().into_iter().flatten().filter_map(|row| row["id"].as_i64()).collect::<Vec<_>>();
            Ok(ToolOutput { display: json!({"kind": "rows", "rowIds": ids, "total": output["total"]}), output })
        }
        "view_images" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                row_ids: Vec<i64>,
                #[serde(default)]
                detail: Option<Value>,
                #[serde(default)]
                crop: Option<Value>,
            }
            let args: Args = parse(arguments)?;
            let mut ids = Vec::new();
            for id in args.row_ids {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            if ids.is_empty() {
                return Err("rowIds 不能为空".into());
            }
            if ids.len() > vision::MAX_IMAGES {
                return Err(format!("一次最多查看 {} 张，请分批查看", vision::MAX_IMAGES));
            }
            if args.detail.is_some() || args.crop.is_some() {
                if ids.len() != 1 {
                    return Err("detail 和 crop 只能在只看 1 张图时使用".into());
                }
                let original: vision::OriginalView = parse(&json!({"detail": args.detail.unwrap_or(json!("standard")), "crop": args.crop}))?;
                let (url, info) = vision::render_original(&runtime, ids[0], &original)?;
                app.state::<Engine>().vision.put(call_id, vec![url]);
                return Ok(ToolOutput {
                    output: json!({"viewed": ids, "original": info, "note": "原图视图在下一条消息里；裁切坐标基于原图像素"}),
                    display: json!({"kind": "images", "rowIds": ids, "missing": [], "view": original}),
                });
            }
            let rendered = vision::render(&runtime, &ids)?;
            app.state::<Engine>().vision.put(call_id, rendered.images.clone());
            let layout = if ids.len() > vision::SEPARATE_LIMIT { "拼成一张总览图，每格下方标着 #图片编号" } else { "逐张附上" };
            Ok(ToolOutput {
                output: json!({"viewed": ids, "missing": rendered.missing, "layout": layout, "note": "图片在下一条消息里"}),
                display: json!({"kind": "images", "rowIds": ids, "missing": rendered.missing}),
            })
        }
        "analyze_text" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                target: Target,
                pattern: String,
                #[serde(default)]
                is_regex: bool,
                #[serde(default)]
                case_sensitive: bool,
                #[serde(default)]
                fields: Vec<String>,
            }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| {
                db.agent_analyze_text(&args.target, &view, &args.pattern, args.is_regex, args.case_sensitive, &args.fields)
            })?)
        }
        "summarize_rows" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                target: Target,
                limit: Option<usize>,
                #[serde(default)]
                compare_library: bool,
                #[serde(default)]
                sort_by: PromptStatsSort,
                #[serde(default)]
                lists: Vec<String>,
            }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| {
                db.agent_summarize_rows(
                    &args.target,
                    &view,
                    args.limit.unwrap_or(DEFAULT_PROMPT_STATS_LIMIT),
                    args.compare_library,
                    args.sort_by,
                    &args.lists,
                )
            })?)
        }
        "list_artist_strings" => {
            #[derive(Deserialize)]
            struct Args {
                target: Target,
                #[serde(default)]
                contains: Option<String>,
                #[serde(default)]
                limit: Option<usize>,
                #[serde(default)]
                offset: usize,
            }
            let args: Args = parse(arguments)?;
            plain(runtime.agent_db(false, |db| {
                db.agent_artist_strings(&args.target, &view, args.contains.as_deref(), args.limit.unwrap_or(30), args.offset)
            })?)
        }
        "preview_changes" => {
            #[derive(Deserialize)]
            struct Args {
                title: String,
                target: Target,
                actions: Vec<Action>,
            }
            let args: Args = parse(arguments)?;
            let report = runtime.agent_db(false, |db| {
                db.agent_preview(conversation_id, &args.title, &args.target, &args.actions, &view)
            })?;
            Ok(preview_output(serde_json::to_value(&report).map_err(|e| e.to_string())?))
        }
        "preview_library_change" => {
            #[derive(Deserialize)]
            struct Args { title: String, operation: LibraryOp }
            let args: Args = parse(arguments)?;
            let report = runtime.agent_db(false, |db| db.agent_preview_library(conversation_id, &args.title, &args.operation))?;
            Ok(preview_output(serde_json::to_value(&report).map_err(|e| e.to_string())?))
        }
        "list_artist_prefix_candidates" => plain(runtime.agent_db(false, |db| {
            let preview = db.preview_auto_artist_prefix().map_err(|e| e.to_string())?;
            serde_json::to_value(preview).map_err(|e| e.to_string())
        })?),
        "preview_rule_on_library" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args { rule_id: i64, #[serde(default)] target: Option<Target> }
            let args: Args = parse(arguments)?;
            let report = runtime.agent_db(false, |db| {
                let rule = db.list_automation_rules().map_err(|e| e.to_string())?.into_iter()
                    .find(|rule| rule.id == args.rule_id).ok_or_else(|| format!("自动规则 #{} 不存在", args.rule_id))?;
                let names = db.list_groups().map_err(|e| e.to_string())?.into_iter().map(|group| (group.id, group.name)).collect::<HashMap<_, _>>();
                let mut actions = Vec::new();
                for action in &rule.actions {
                    actions.extend(Action::from_rule(action, &names)?);
                }
                if actions.is_empty() {
                    return Err("这条规则没有可以在现有图片上执行的动作".into());
                }
                let mut target = args.target.unwrap_or_else(Target::all);
                target.rule_id = Some(rule.id);
                db.agent_preview(conversation_id, &format!("在图库上试跑自动规则「{}」", rule.name), &target, &actions, &view)
            })?;
            Ok(preview_output(serde_json::to_value(&report).map_err(|e| e.to_string())?))
        }
        "apply_changes" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                preview_id: String,
            }
            let args: Args = parse(arguments)?;
            let record = runtime.agent_db(true, |db| db.agent_apply(conversation_id, &args.preview_id))?;
            changed(app);
            operation_output(&record, json!({"operationId": record.id, "changedRows": record.changed_rows, "title": record.title, "state": "applied"}))
        }
        "undo_operation" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                operation_id: i64,
            }
            let args: Args = parse(arguments)?;
            let outcome = runtime.agent_db(true, |db| db.agent_undo(args.operation_id))?;
            changed(app);
            let _ = app.emit("automation-rules://changed", ());
            let value = serde_json::to_value(&outcome).map_err(|e| e.to_string())?;
            Ok(ToolOutput { display: json!({"kind": "undo", "outcome": value}), output: value })
        }
        "list_operations" => {
            let limit = arguments["limit"].as_u64().unwrap_or(10).clamp(1, 100) as usize;
            let list = runtime.agent_db(false, |db| db.agent_operations(limit))?;
            plain(json!(list.iter().map(|item| json!({
                "id": item.id, "title": item.title, "kind": item.kind, "changedRows": item.changed_rows,
                "state": item.state, "createdAt": crate::db::agent::local_time(Some(&item.created_at)), "undoMessage": item.undo_message,
            })).collect::<Vec<_>>()))
        }
        "set_main_selection" | "show_selected_images" => {
            let show_only = name == "show_selected_images";
            let args: TargetArgs = parse(arguments)?;
            let (ids, notes) = runtime.agent_db(false, |db| {
                let resolved = db.agent_resolve(&args.target, &view)?;
                Ok((resolved.rows.iter().map(|row| row.id).collect::<Vec<_>>(), resolved.notes))
            })?;
            if ids.is_empty() && !show_only {
                return Err("范围内没有图片，主窗口的选择没有改变".into());
            }
            app.emit_to("main", "agent://main-selection", json!({"rowIds": ids, "showOnly": show_only})).map_err(|e| e.to_string())?;
            Ok(ToolOutput {
                output: json!({"selected": ids.len(), "notes": notes}),
                display: json!({"kind": "selection", "count": ids.len(), "notes": notes}),
            })
        }
        "set_main_filters" => {
            let mut patch = serde_json::Map::new();
            for key in ["search", "tags", "tagMode", "artistFilter", "dedupe", "singleArtistOnly", "hasVibe", "untaggedOnly", "hideGrouped"] {
                if let Some(value) = arguments.get(key) {
                    patch.insert(key.into(), value.clone());
                }
            }
            let clear = arguments["clear"].as_bool().unwrap_or(false);
            if patch.is_empty() && !clear {
                return Err("没有提供任何筛选条件".into());
            }
            app.emit("agent://main-filters", json!({"clear": clear, "patch": patch})).map_err(|e| e.to_string())?;
            Ok(ToolOutput {
                output: json!({"ok": true, "note": "已发送到主窗口；需要结果数量请再调用 get_main_window_state"}),
                display: json!({"kind": "filters", "clear": clear, "patch": patch}),
            })
        }
        "control_main_window" => {
            #[derive(Deserialize, serde::Serialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Compare { row_id: i64, #[serde(default)] target_row_id: Option<i64> }
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Args {
                #[serde(default)] view_mode: Option<String>,
                #[serde(default)] sort: Option<RowSort>,
                #[serde(default)] duplicates_mode: Option<String>,
                #[serde(default)] open_group_id: Option<i64>,
                #[serde(default)] open_detail_row_id: Option<i64>,
                #[serde(default)] locate_row_id: Option<i64>,
                #[serde(default)] compare: Option<Compare>,
            }
            let args: Args = parse(arguments)?;
            if args.view_mode.is_none() && args.sort.is_none() && args.duplicates_mode.is_none() && args.open_group_id.is_none()
                && args.open_detail_row_id.is_none() && args.locate_row_id.is_none() && args.compare.is_none() {
                return Err("没有提供任何要做的操作".into());
            }
            let rows = [args.open_detail_row_id, args.locate_row_id, args.compare.as_ref().map(|c| c.row_id),
                args.compare.as_ref().and_then(|c| c.target_row_id)].into_iter().flatten().collect::<Vec<_>>();
            runtime.agent_db(false, |db| {
                if !rows.is_empty() {
                    let found = db.get_rows_by_ids(&rows).map_err(|e| e.to_string())?;
                    if let Some(missing) = rows.iter().find(|id| !found.iter().any(|row| row.id == **id)) {
                        return Err(format!("图片 #{missing} 不存在"));
                    }
                }
                if let Some(group) = args.open_group_id
                    && !db.list_groups().map_err(|e| e.to_string())?.iter().any(|item| item.id == group) {
                    return Err(format!("分组 #{group} 不存在"));
                }
                Ok(())
            })?;
            if args.compare.as_ref().is_some_and(|c| c.target_row_id == Some(c.row_id)) {
                return Err("A 和 B 不能是同一张图".into());
            }
            if args.view_mode.as_deref().is_some_and(|mode| !["gallery", "table", "group", "duplicates", "materials", "promptDocs"].contains(&mode)) {
                return Err("viewMode 只能是 gallery、table、group、duplicates、materials、promptDocs".into());
            }
            if args.duplicates_mode.as_deref().is_some_and(|mode| !["artists", "positivePrompt", "vibes"].contains(&mode)) {
                return Err("duplicatesMode 只能是 artists、positivePrompt、vibes".into());
            }
            let sort = args.sort.map(|sort| match sort {
                RowSort::Oldest => "timeAsc",
                RowSort::Newest => "timeDesc",
                RowSort::RecentlyUpdated => "recentlyUpdated",
            });
            let payload = json!({
                "viewMode": args.view_mode, "sort": sort, "duplicatesMode": args.duplicates_mode,
                "openGroupId": args.open_group_id, "openDetailRowId": args.open_detail_row_id,
                "locateRowId": args.locate_row_id, "compare": args.compare,
            });
            app.emit_to("main", "agent://main-control", payload.clone()).map_err(|e| e.to_string())?;
            Ok(ToolOutput {
                output: json!({"ok": true, "note": "已发送到主窗口；需要确认结果可再调用 get_main_window_state"}),
                display: json!({"kind": "control", "request": payload}),
            })
        }
        "create_automation_rule" => {
            let draft: AutomationRuleDraft = parse(&arguments["rule"])?;
            let record = runtime.agent_db(true, |db| {
                let rule = db.create_automation_rule(&draft).map_err(|e| e.to_string())?;
                db.agent_record_rule(conversation_id, &rule)
            })?;
            rules_changed(app);
            operation_output(&record, json!({"operationId": record.id, "created": record.title}))
        }
        "update_automation_rule" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args { rule_id: i64, patch: RulePatch }
            let args: Args = parse(arguments)?;
            let record = runtime.agent_db(true, |db| db.agent_update_rule(conversation_id, args.rule_id, args.patch))?;
            rules_changed(app);
            operation_output(&record, json!({"operationId": record.id, "done": record.title}))
        }
        "delete_automation_rule" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args { rule_id: i64 }
            let args: Args = parse(arguments)?;
            let record = runtime.agent_db(true, |db| db.agent_delete_rule(conversation_id, args.rule_id))?;
            rules_changed(app);
            operation_output(&record, json!({"operationId": record.id, "done": record.title}))
        }
        other => Err(format!("没有名为 {other} 的工具")),
    }
}

/// Names of the tools offered at this point of a conversation, beyond the core set.
pub fn active_tools(messages: &[crate::db::agent::Message]) -> HashSet<String> {
    let mut names = HashSet::new();
    for message in messages {
        match message.kind.as_str() {
            "tool_call" => {
                if let Some(name) = message.content["name"].as_str() {
                    names.insert(name.to_owned());
                }
            }
            "tool_result" if message.content["name"] == "load_tools" && message.content["ok"] == true => {
                for key in message.content["display"]["groups"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    if let Some(group) = GROUPS.iter().find(|group| group.key == key) {
                        names.extend(group.tools.iter().map(|name| (*name).to_owned()));
                    }
                }
            }
            "user" => {
                let images = message.content["attachments"].as_array().into_iter().flatten()
                    .any(|file| file["mime"].as_str().is_some_and(|mime| mime.starts_with("image/")));
                if images && let Some(group) = GROUPS.iter().find(|group| group.key == "attachments") {
                    names.extend(group.tools.iter().map(|name| (*name).to_owned()));
                }
            }
            _ => {}
        }
    }
    names.retain(|name| CORE.contains(&name.as_str()) || LEGACY.contains(&name.as_str()) || group_of(name).is_some());
    names
}
