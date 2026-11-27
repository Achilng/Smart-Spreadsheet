//! Materials and prompt docs written by the agent. New ones are created directly and can be
//! undone; edits to existing ones are previewed first and run after the user confirms.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use super::markdown;
use super::tools::ToolOutput;
use crate::app::AppRuntime;
use crate::app::materials::{cover_pair, decode, decode_bytes};
use crate::db::agent::{OperationRecord, UndoOutcome, is_content_kind, new_preview_id};
use crate::db::materials::{Material, MaterialDraft, MaterialVersionDraft};
use crate::storage::PromptDocDetail;

const MAX_VERSIONS: usize = 128;
/// Characters of text shown on a card per field.
const EXCERPT: usize = 800;

type Cover = (Vec<u8>, Vec<u8>);

fn parse<T: for<'de> Deserialize<'de>>(arguments: &Value) -> Result<T, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| format!("参数格式不对：{error}"))
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn excerpt(value: &str, max: usize) -> String {
    let value = value.trim();
    let mut chars = value.chars();
    let head = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() { format!("{head}…") } else { head }
}

fn quoted(names: &[String]) -> String {
    names.iter().map(|name| format!("「{name}」")).collect::<String>()
}

fn changed(app: &AppHandle, kind: &str, id: Value) {
    let _ = app.emit("agent://content-changed", json!({"kind": kind, "id": id}));
    let _ = app.emit("agent://operations-changed", ());
}

// ---- 展示图来源 ----

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoverSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    row_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attachment_number: Option<u8>,
}

impl CoverSource {
    fn describe(&self) -> String {
        match (self.row_id, self.message_id, self.attachment_number) {
            (Some(row), _, _) => format!("图库图片 #{row}"),
            (_, Some(_), Some(number)) => format!("聊天里上传的第 {number} 张附件"),
            _ => "（未知来源）".into(),
        }
    }
}

fn load_cover(runtime: &AppRuntime, conversation_id: &str, source: &CoverSource) -> Result<Cover, String> {
    let image = match (source.row_id, source.message_id, source.attachment_number) {
        (Some(row_id), None, None) => {
            let directory = runtime.active_directory().map_err(text)?;
            let locator = runtime.agent_db(false, |db| db.row_image_locator(row_id).map_err(|_| format!("图片 #{row_id} 不存在")))?;
            let path = crate::storage::resolve_image_source(&directory, &locator)
                .ok_or_else(|| format!("图片 #{row_id} 的原文件和库内副本都读不到，换一张图"))?;
            decode(&path).map_err(|error| format!("图片 #{row_id} 读不出来：{error}"))?
        }
        (None, Some(message_id), Some(number)) => {
            let (_, data) = runtime.agent_db(false, |db| db.agent_image_attachment(conversation_id, message_id, number))?;
            let bytes = STANDARD.decode(data).map_err(text)?;
            decode_bytes(&bytes).map_err(|error| format!("这个附件读不出来：{error}"))?
        }
        _ => return Err("展示图 cover 只能二选一：{rowId}（图库图片）或 {messageId, attachmentNumber}（聊天附件）".into()),
    };
    cover_pair(&image)
}

// ---- 入口 ----

pub fn run(app: &AppHandle, conversation_id: &str, name: &str, arguments: &Value) -> Result<ToolOutput, String> {
    let runtime = app.state::<AppRuntime>();
    match name {
        "create_material" => create_material(app, &runtime, conversation_id, parse(arguments)?),
        "create_prompt_doc" => create_prompt_doc(app, &runtime, conversation_id, parse(arguments)?),
        "preview_content_change" => preview(&runtime, conversation_id, parse(arguments)?),
        other => Err(format!("没有名为 {other} 的工具")),
    }
}

fn operation_output(record: &OperationRecord, output: Value) -> Result<ToolOutput, String> {
    Ok(ToolOutput { output, display: json!({"kind": "operation", "operation": serde_json::to_value(record).map_err(text)?}) })
}

// ---- 新建素材 ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NewVersion {
    #[serde(default)]
    name: String,
    text: String,
    #[serde(default)]
    cover: Option<CoverSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateMaterial {
    title: String,
    versions: Vec<NewVersion>,
    #[serde(default)]
    tags: Vec<String>,
    cover: CoverSource,
}

fn version_name(name: &str, index: usize) -> String {
    let name = name.trim();
    if name.is_empty() { format!("版本 {}", index + 1) } else { name.to_owned() }
}

fn create_material(app: &AppHandle, runtime: &AppRuntime, conversation_id: &str, args: CreateMaterial) -> Result<ToolOutput, String> {
    let title = args.title.trim().to_owned();
    if title.is_empty() {
        return Err("素材标题不能为空".into());
    }
    if args.versions.is_empty() || args.versions.len() > MAX_VERSIONS {
        return Err(format!("versions 要有 1 至 {MAX_VERSIONS} 个版本"));
    }
    let cover = load_cover(runtime, conversation_id, &args.cover)?;
    let mut version_images = Vec::with_capacity(args.versions.len());
    for version in &args.versions {
        version_images.push(version.cover.as_ref().map(|source| load_cover(runtime, conversation_id, source)).transpose()?);
    }
    if version_images.iter().all(Option::is_none) {
        version_images.clear();
    }
    let draft = MaterialDraft {
        id: None,
        title: title.clone(),
        text: args.versions[0].text.clone(),
        tags: args.tags.clone(),
        image_path: None,
        versions: Some(args.versions.iter().enumerate().map(|(index, version)| MaterialVersionDraft {
            id: None, name: version_name(&version.name, index), text: version.text.clone(), image_path: None, image_source_id: None,
        }).collect()),
    };
    let mut lines = vec![format!("新建素材「{title}」，共 {} 个版本", args.versions.len()), format!("展示图用的是{}", args.cover.describe())];
    if !args.tags.is_empty() {
        lines.push(format!("TAG：{}", args.tags.join("、")));
    }
    for (index, version) in args.versions.iter().enumerate() {
        if let Some(source) = &version.cover {
            lines.push(format!("版本「{}」单独用{}当图片", version_name(&version.name, index), source.describe()));
        }
    }
    let diffs = args.versions.iter().enumerate()
        .map(|(index, version)| json!({"label": format!("版本「{}」", version_name(&version.name, index)), "after": excerpt(&version.text, EXCERPT)}))
        .collect::<Vec<_>>();
    let record = runtime.agent_db(false, |db| {
        let created_tags = db.agent_new_tags(&args.tags)?;
        let material = db.save_material_versions(&draft, Some((&cover.0, &cover.1)), &version_images).map_err(text)?;
        let report = json!({
            "title": format!("新建素材「{}」", material.title),
            "content": {"type": "material", "action": "create", "id": material.id, "name": material.title,
                "lines": lines, "diffs": diffs, "cover": args.cover},
            "materialId": material.id, "updatedAt": material.updated_at, "state": material_state(&material), "createdTags": created_tags,
        });
        db.agent_record_content(conversation_id, "material_create", &format!("新建素材「{}」", material.title), &report, None)
    })?;
    let material_id = record.report["materialId"].clone();
    changed(app, "material", material_id.clone());
    operation_output(&record, json!({
        "operationId": record.id, "materialId": material_id, "title": title, "versions": args.versions.len(),
        "note": "素材已经建好，用户可在素材页看到；撤销用 undo_operation。",
    }))
}

// ---- 新建提示词文档 ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateDoc {
    title: String,
    markdown: String,
}

fn create_prompt_doc(app: &AppHandle, runtime: &AppRuntime, conversation_id: &str, args: CreateDoc) -> Result<ToolOutput, String> {
    if args.markdown.trim().is_empty() {
        return Err("文档正文 markdown 不能为空".into());
    }
    let content = markdown::to_tiptap(&args.markdown);
    let plain = markdown::plain_text(&content);
    let created = runtime.create_prompt_doc(&args.title).map_err(text)?;
    let saved = match runtime.save_prompt_doc(&created.id, &args.title, &content, &plain) {
        Ok(saved) => saved,
        Err(error) => {
            let _ = runtime.delete_prompt_doc(&created.id);
            return Err(format!("文档写入失败：{error}"));
        }
    };
    let title = format!("新建提示词文档「{}」", saved.title);
    let state = load_doc(runtime, &saved.id).map(|doc| doc_state(&doc)).unwrap_or(Value::Null);
    let report = json!({
        "title": title,
        "content": {"type": "doc", "action": "create", "id": saved.id, "name": saved.title,
            "lines": [format!("新建提示词文档「{}」，正文约 {} 字", saved.title, plain.chars().count())],
            "diffs": [{"label": "正文", "after": excerpt(&plain, EXCERPT)}]},
        "docId": saved.id, "updatedAt": saved.updated_at, "state": state,
    });
    let record = runtime.agent_db(false, |db| db.agent_record_content(conversation_id, "doc_create", &title, &report, None))?;
    changed(app, "doc", json!(saved.id));
    operation_output(&record, json!({
        "operationId": record.id, "docId": saved.id, "title": saved.title,
        "note": "文档已经建好，用户可在主窗口的「文档」页看到；撤销用 undo_operation。",
    }))
}

// ---- 修改已有素材 / 文档（先预演） ----

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum DocMode {
    #[default]
    Append,
    Replace,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VersionPatch {
    version_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum ContentOp {
    AddMaterialVersion {
        material_id: i64,
        #[serde(default)]
        name: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cover: Option<CoverSource>,
    },
    UpdateMaterial {
        material_id: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<String>>,
        #[serde(default)]
        add_tags: Vec<String>,
        #[serde(default)]
        remove_tags: Vec<String>,
        #[serde(default)]
        versions: Vec<VersionPatch>,
    },
    EditPromptDoc {
        doc_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        markdown: Option<String>,
        #[serde(default)]
        mode: DocMode,
    },
}

#[derive(Deserialize)]
struct PreviewArgs {
    title: String,
    operation: ContentOp,
}

/// A version as it will be saved: `id` is None for the one being added.
struct PlannedVersion {
    id: Option<i64>,
    name: String,
    text: String,
    keep_image: bool,
}

struct MaterialPlan {
    title: String,
    tags: Vec<String>,
    versions: Vec<PlannedVersion>,
    /// Picture for the added version, encoded only when the change runs.
    new_cover: Option<CoverSource>,
    lines: Vec<String>,
    diffs: Vec<Value>,
}

impl MaterialPlan {
    fn draft(&self, material_id: i64) -> MaterialDraft {
        MaterialDraft {
            id: Some(material_id),
            title: self.title.clone(),
            text: self.versions.first().map(|version| version.text.clone()).unwrap_or_default(),
            tags: self.tags.clone(),
            image_path: None,
            versions: Some(self.versions.iter().map(|version| MaterialVersionDraft {
                id: version.id, name: version.name.clone(), text: version.text.clone(), image_path: None,
                image_source_id: version.id.filter(|_| version.keep_image),
            }).collect()),
        }
    }
}

/// The material as it is now, as a plan that would save it unchanged.
fn current_plan(material: &Material) -> MaterialPlan {
    MaterialPlan {
        title: material.title.clone(),
        tags: material.tags.clone(),
        versions: material.versions.iter().map(|version| PlannedVersion {
            id: Some(version.id), name: version.name.clone(), text: version.text.clone(), keep_image: version.has_image,
        }).collect(),
        new_cover: None,
        lines: Vec::new(),
        diffs: Vec::new(),
    }
}

fn plan_material(material: &Material, op: &ContentOp) -> Result<MaterialPlan, String> {
    let mut plan = current_plan(material);
    match op {
        ContentOp::AddMaterialVersion { name, text, cover, .. } => {
            if plan.versions.len() >= MAX_VERSIONS {
                return Err(format!("素材「{}」已经有 {MAX_VERSIONS} 个版本，不能再加", material.title));
            }
            let name = version_name(name, plan.versions.len());
            plan.lines.push(format!("给素材「{}」新增版本「{name}」（现有 {} 个版本不变）", material.title, plan.versions.len()));
            match cover {
                Some(source) => plan.lines.push(format!("新版本的图片用{}", source.describe())),
                None => plan.lines.push("新版本没有单独的图片，显示素材的展示图".into()),
            }
            plan.diffs.push(json!({"label": format!("新版本「{name}」"), "after": excerpt(text, EXCERPT)}));
            plan.versions.push(PlannedVersion { id: None, name, text: text.clone(), keep_image: false });
            plan.new_cover = cover.clone();
        }
        ContentOp::UpdateMaterial { title, tags, add_tags, remove_tags, versions, .. } => {
            if let Some(title) = title.as_deref().map(str::trim).filter(|title| *title != material.title) {
                if title.is_empty() {
                    return Err("素材标题不能改成空的".into());
                }
                plan.lines.push(format!("标题「{}」改成「{title}」", material.title));
                plan.title = title.to_owned();
            }
            let mut next = tags.clone().unwrap_or_else(|| material.tags.clone());
            next.retain(|tag| !remove_tags.contains(tag));
            for tag in add_tags {
                if !next.contains(tag) {
                    next.push(tag.clone());
                }
            }
            let added = next.iter().filter(|tag| !material.tags.contains(tag)).cloned().collect::<Vec<_>>();
            let removed = material.tags.iter().filter(|tag| !next.contains(tag)).cloned().collect::<Vec<_>>();
            if !added.is_empty() { plan.lines.push(format!("加上 TAG{}", quoted(&added))); }
            if !removed.is_empty() { plan.lines.push(format!("去掉 TAG{}", quoted(&removed))); }
            plan.tags = next;
            for patch in versions {
                let Some(version) = plan.versions.iter_mut().find(|version| version.id == Some(patch.version_id)) else {
                    return Err(format!("素材「{}」里没有版本 #{}，先用 read_library_item 看一下现有版本", material.title, patch.version_id));
                };
                if let Some(name) = patch.name.as_deref().map(str::trim).filter(|name| !name.is_empty() && *name != version.name) {
                    plan.lines.push(format!("版本「{}」改名为「{name}」", version.name));
                    version.name = name.to_owned();
                }
                if let Some(text) = patch.text.as_ref().filter(|text| **text != version.text) {
                    plan.diffs.push(json!({"label": format!("版本「{}」的正文", version.name), "before": excerpt(&version.text, EXCERPT), "after": excerpt(text, EXCERPT)}));
                    version.text = text.clone();
                }
            }
            let texts = plan.diffs.len();
            if texts > 0 { plan.lines.push(format!("改写 {texts} 个版本的正文")); }
            if plan.lines.is_empty() {
                return Err("修改后的素材和现在完全一样".into());
            }
        }
        ContentOp::EditPromptDoc { .. } => unreachable!("doc op"),
    }
    Ok(plan)
}

struct DocPlan {
    title: String,
    content: Value,
    plain: String,
    lines: Vec<String>,
    diffs: Vec<Value>,
    warnings: Vec<String>,
}

fn count_images(node: &Value) -> usize {
    usize::from(node["type"] == "image") + node["content"].as_array().into_iter().flatten().map(count_images).sum::<usize>()
}

fn plan_doc(doc: &PromptDocDetail, op: &ContentOp) -> Result<DocPlan, String> {
    let ContentOp::EditPromptDoc { title, markdown: text, mode, .. } = op else { unreachable!("material op") };
    let mut plan = DocPlan { title: doc.title.clone(), content: doc.content.clone(), plain: doc.plain_text.clone(), lines: Vec::new(), diffs: Vec::new(), warnings: Vec::new() };
    if let Some(title) = title.as_deref().map(str::trim).filter(|title| !title.is_empty() && *title != doc.title) {
        plan.lines.push(format!("标题「{}」改成「{title}」", doc.title));
        plan.title = title.to_owned();
    }
    if let Some(text) = text.as_deref().filter(|text| !text.trim().is_empty()) {
        let addition = markdown::to_tiptap(text);
        let addition_plain = markdown::plain_text(&addition);
        if *mode == DocMode::Replace {
            plan.content = addition;
            plan.plain = addition_plain;
            plan.lines.push(format!("把文档「{}」的正文整体换成新内容（约 {} 字）", doc.title, plan.plain.chars().count()));
            plan.diffs.push(json!({"label": "正文", "before": excerpt(&doc.plain_text, EXCERPT), "after": excerpt(&plan.plain, EXCERPT)}));
            let images = count_images(&doc.content);
            if images > 0 {
                plan.warnings.push(format!("原文里有 {images} 张图片，整体替换后它们不再显示（图片文件还在，撤销可以恢复）"));
            }
        } else {
            plan.content = markdown::append(&doc.content, &addition);
            plan.plain = markdown::plain_text(&plan.content);
            plan.lines.push(format!("在文档「{}」末尾追加一段内容（约 {} 字），原有内容不变", doc.title, addition_plain.chars().count()));
            plan.diffs.push(json!({"label": "追加的内容", "after": excerpt(&addition_plain, EXCERPT)}));
        }
    }
    if plan.lines.is_empty() {
        return Err("修改后的文档和现在完全一样（要改正文请给 markdown，改标题给 title）".into());
    }
    Ok(plan)
}

fn load_material(runtime: &AppRuntime, id: i64) -> Result<Material, String> {
    runtime.agent_db(false, |db| db.material(id).map_err(|_| format!("素材 #{id} 不存在")))
}

fn load_doc(runtime: &AppRuntime, id: &str) -> Result<PromptDocDetail, String> {
    runtime.load_prompt_doc(id).map_err(|_| format!("提示词文档 {id} 不存在"))
}

/// The parts of a material a change reads. While they stay the same, the change still does exactly
/// what its preview showed, whatever else was edited in between.
fn material_basis(material: &Material, op: &ContentOp) -> Value {
    match op {
        ContentOp::AddMaterialVersion { name, .. } => json!({"name": version_name(name, material.versions.len())}),
        ContentOp::UpdateMaterial { title, tags, add_tags, remove_tags, versions, .. } => json!({
            "title": title.as_ref().map(|_| &material.title),
            "tags": tags.as_ref().map(|_| &material.tags),
            "addTags": add_tags.iter().map(|tag| material.tags.contains(tag)).collect::<Vec<_>>(),
            "removeTags": remove_tags.iter().map(|tag| material.tags.contains(tag)).collect::<Vec<_>>(),
            "versions": versions.iter()
                .map(|patch| material.versions.iter().find(|version| version.id == patch.version_id).map(|version| json!([version.name, version.text])))
                .collect::<Vec<_>>(),
        }),
        ContentOp::EditPromptDoc { .. } => unreachable!("doc op"),
    }
}

/// Appending keeps whatever the doc holds by then, so only a replacement depends on the old body.
fn doc_basis(doc: &PromptDocDetail, op: &ContentOp) -> Value {
    let ContentOp::EditPromptDoc { title, markdown: text, mode, .. } = op else { unreachable!("material op") };
    let replaces = *mode == DocMode::Replace && text.as_deref().is_some_and(|text| !text.trim().is_empty());
    json!({"title": title.as_ref().map(|_| &doc.title), "content": replaces.then_some(&doc.content)})
}

/// What a material looks like to the user, to tell whether anything changed since an operation.
/// Unlike `updated_at`, undoing a later operation brings it back.
fn material_state(material: &Material) -> Value {
    let mut tags = material.tags.clone();
    tags.sort();
    json!({
        "title": material.title, "tags": tags,
        "versions": material.versions.iter().map(|version| json!([version.id, version.name, version.text, version.has_image])).collect::<Vec<_>>(),
    })
}

fn doc_state(doc: &PromptDocDetail) -> Value {
    json!({"title": doc.title, "content": doc.content})
}

/// Whether the item still looks as it did right after the operation. Records written before
/// snapshots existed only have the modification time.
fn unchanged_since(state: &Value, current_state: Value, updated_at: &Value, current_updated_at: &str) -> bool {
    if state.is_null() { updated_at.as_str() == Some(current_updated_at) } else { *state == current_state }
}

fn drifted(name: &str) -> String {
    format!("预览之后，「{name}」里这次要改的内容又被改过了，按这张预览执行会和看到的不一样，所以没有执行。请重新预览一次（可以直接让助手重做预览）")
}

fn preview(runtime: &AppRuntime, conversation_id: &str, args: PreviewArgs) -> Result<ToolOutput, String> {
    let (kind, id, name, updated_at, basis, lines, diffs, warnings) = match &args.operation {
        ContentOp::AddMaterialVersion { material_id, .. } | ContentOp::UpdateMaterial { material_id, .. } => {
            let material = load_material(runtime, *material_id)?;
            let plan = plan_material(&material, &args.operation)?;
            if let Some(source) = &plan.new_cover {
                // Fail now rather than after the user confirms.
                load_cover(runtime, conversation_id, source)?;
            }
            let basis = material_basis(&material, &args.operation);
            ("material", json!(material.id), material.title.clone(), material.updated_at.clone(), basis, plan.lines, plan.diffs, Vec::new())
        }
        ContentOp::EditPromptDoc { doc_id, .. } => {
            let doc = load_doc(runtime, doc_id)?;
            let plan = plan_doc(&doc, &args.operation)?;
            let basis = doc_basis(&doc, &args.operation);
            ("doc", json!(doc.id), doc.title.clone(), doc.updated_at.clone(), basis, plan.lines, plan.diffs, plan.warnings)
        }
    };
    let preview_id = new_preview_id();
    let title = args.title.trim().to_owned();
    let report = json!({
        "previewId": preview_id, "title": title,
        "content": {"type": kind, "action": "edit", "id": id, "name": name, "lines": lines, "diffs": diffs},
        "warnings": warnings,
        // Fields the row preview card also reads.
        "summary": [], "newTags": [], "newGroups": [], "changedRows": 0, "unchangedRows": 0,
    });
    let plan = json!({"kind": "content", "op": args.operation, "expectedUpdatedAt": updated_at, "basis": basis, "title": title});
    runtime.agent_db(false, |db| db.agent_store_content_preview(conversation_id, &preview_id, &plan, &report))?;
    Ok(ToolOutput {
        output: json!({"previewId": preview_id, "target": {"type": kind, "id": id, "name": name}, "changes": lines, "warnings": warnings,
            "note": "预演卡片已显示给用户；用户同意后用 apply_changes(previewId) 执行，可撤销。"}),
        display: json!({"kind": "preview", "report": report}),
    })
}

/// Run a preview: content previews here, every other kind in the database.
pub fn apply(app: &AppHandle, conversation_id: &str, preview_id: &str) -> Result<OperationRecord, String> {
    let runtime = app.state::<AppRuntime>();
    let stored = runtime.agent_db(false, |db| db.agent_stored_preview(preview_id))?;
    if stored.as_ref().is_none_or(|stored| stored.plan["kind"] != "content") {
        let record = runtime.agent_db(true, |db| db.agent_apply(conversation_id, preview_id))?;
        let _ = app.emit("agent://library-changed", ());
        let _ = app.emit("agent://operations-changed", ());
        return Ok(record);
    }
    let stored = stored.expect("checked above");
    if stored.conversation_id != conversation_id {
        return Err("这个预览不属于当前对话".into());
    }
    if stored.used {
        return Err("这个预览已经执行过了；如需再次修改请重新预览".into());
    }
    let op: ContentOp = serde_json::from_value(stored.plan["op"].clone()).map_err(text)?;
    let expected = stored.plan["expectedUpdatedAt"].as_str().unwrap_or_default();
    // Previews stored before `basis` existed fall back to the modification time.
    let still_valid = |basis: Value, updated_at: &str| match stored.plan.get("basis") {
        Some(stored_basis) => *stored_basis == basis,
        None => updated_at == expected,
    };
    let title = stored.plan["title"].as_str().unwrap_or("修改").to_owned();
    let mut report = stored.report.clone();
    let (kind, record) = match &op {
        ContentOp::AddMaterialVersion { material_id, .. } | ContentOp::UpdateMaterial { material_id, .. } => {
            let before = load_material(&runtime, *material_id)?;
            if !still_valid(material_basis(&before, &op), &before.updated_at) {
                return Err(drifted(&before.title));
            }
            let plan = plan_material(&before, &op)?;
            let cover = plan.new_cover.as_ref().map(|source| load_cover(&runtime, conversation_id, source)).transpose()?;
            let version_images: Vec<Option<Cover>> = match cover {
                Some(cover) => plan.versions.iter().map(|version| version.id.is_none().then(|| cover.clone())).collect(),
                None => Vec::new(),
            };
            let record = runtime.agent_db(false, |db| {
                let created_tags = db.agent_new_tags(&plan.tags)?;
                let after = db.save_material_versions(&plan.draft(before.id), None, &version_images).map_err(text)?;
                report["undo"] = json!({"materialId": before.id, "before": before, "updatedAt": after.updated_at,
                    "after": material_state(&after), "createdTags": created_tags});
                db.agent_record_content(conversation_id, "material_edit", &title, &report, Some(preview_id))
            })?;
            (("material", json!(material_id)), record)
        }
        ContentOp::EditPromptDoc { doc_id, .. } => {
            let before = load_doc(&runtime, doc_id)?;
            if !still_valid(doc_basis(&before, &op), &before.updated_at) {
                return Err(drifted(&before.title));
            }
            let plan = plan_doc(&before, &op)?;
            let after = runtime.save_prompt_doc(doc_id, &plan.title, &plan.content, &plan.plain).map_err(text)?;
            let state = load_doc(&runtime, doc_id).map(|saved| doc_state(&saved)).unwrap_or(Value::Null);
            report["undo"] = json!({"docId": doc_id, "title": before.title, "content": before.content, "plainText": before.plain_text,
                "updatedAt": after.updated_at, "after": state});
            let record = runtime.agent_db(false, |db| db.agent_record_content(conversation_id, "doc_edit", &title, &report, Some(preview_id)))?;
            (("doc", json!(doc_id)), record)
        }
    };
    changed(app, kind.0, kind.1);
    Ok(record)
}

/// Undo an operation: content kinds here, every other kind in the database.
pub fn undo(app: &AppHandle, operation_id: i64) -> Result<(UndoOutcome, Option<OperationRecord>), String> {
    let runtime = app.state::<AppRuntime>();
    let operation = runtime.agent_db(false, |db| db.agent_operation(operation_id))?;
    let Some(operation) = operation.filter(|operation| is_content_kind(&operation.kind)) else {
        let result = runtime.agent_db(true, |db| Ok((db.agent_undo(operation_id)?, db.agent_operation(operation_id)?)))?;
        let _ = app.emit("agent://library-changed", ());
        let _ = app.emit("agent://operations-changed", ());
        if result.1.as_ref().is_some_and(|operation| operation.kind.starts_with("rule")) {
            let _ = app.emit("automation-rules://changed", ());
        }
        return Ok(result);
    };
    if operation.state == "undone" {
        return Err("这次修改已经撤销过了".into());
    }
    let report = &operation.report;
    let (message, kind, id) = match operation.kind.as_str() {
        "material_create" => {
            let id = report["materialId"].as_i64().ok_or("操作记录缺少素材 ID")?;
            let message = match runtime.agent_db(false, |db| Ok(db.material(id).ok()))? {
                None => "这个素材已经不在了".to_owned(),
                Some(current) if !unchanged_since(&report["state"], material_state(&current), &report["updatedAt"], &current.updated_at) => {
                    return Err(format!("素材「{}」建好之后又被改过，为了不丢掉这些修改没有删除；确实不要的话请在素材页手动删除", current.title));
                }
                Some(current) => {
                    runtime.agent_db(false, |db| {
                        db.delete_material(id).map_err(text)?;
                        db.agent_drop_unused_tags(&strings(&report["createdTags"]))
                    })?;
                    format!("已删除助手新建的素材「{}」", current.title)
                }
            };
            (message, "material", json!(id))
        }
        "material_edit" => {
            let undo = &report["undo"];
            let id = undo["materialId"].as_i64().ok_or("操作记录缺少素材 ID")?;
            let current = load_material(&runtime, id).map_err(|_| "这个素材已经被删除，无法改回".to_owned())?;
            if !unchanged_since(&undo["after"], material_state(&current), &undo["updatedAt"], &current.updated_at) {
                return Err(format!("素材「{}」之后又被改过，保留现状", current.title));
            }
            let before = material_from(&undo["before"])?;
            runtime.agent_db(false, |db| {
                db.save_material_versions(&current_plan(&before).draft(id), None, &[]).map_err(text)?;
                db.agent_drop_unused_tags(&strings(&undo["createdTags"]))
            })?;
            (format!("素材「{}」已改回修改前的样子", before.title), "material", json!(id))
        }
        "doc_create" => {
            let id = report["docId"].as_str().ok_or("操作记录缺少文档 ID")?.to_owned();
            let message = match runtime.load_prompt_doc(&id) {
                Err(_) => "这个文档已经不在了".to_owned(),
                Ok(current) if !unchanged_since(&report["state"], doc_state(&current), &report["updatedAt"], &current.updated_at) => {
                    return Err(format!("文档「{}」建好之后又被改过，为了不丢掉这些修改没有删除；确实不要的话请在「文档」页手动删除", current.title));
                }
                Ok(current) => {
                    runtime.delete_prompt_doc(&id).map_err(text)?;
                    format!("已删除助手新建的文档「{}」", current.title)
                }
            };
            (message, "doc", json!(id))
        }
        "doc_edit" => {
            let undo = &report["undo"];
            let id = undo["docId"].as_str().ok_or("操作记录缺少文档 ID")?.to_owned();
            let current = load_doc(&runtime, &id).map_err(|_| "这个文档已经被删除，无法改回".to_owned())?;
            if !unchanged_since(&undo["after"], doc_state(&current), &undo["updatedAt"], &current.updated_at) {
                return Err(format!("文档「{}」之后又被改过，保留现状", current.title));
            }
            let title = undo["title"].as_str().unwrap_or_default();
            runtime.save_prompt_doc(&id, title, &undo["content"], undo["plainText"].as_str().unwrap_or_default()).map_err(text)?;
            (format!("文档「{title}」已改回修改前的样子"), "doc", json!(id))
        }
        other => return Err(format!("不认识的操作类型 {other}")),
    };
    let record = runtime.agent_db(false, |db| {
        db.agent_finish_content_undo(operation_id, &message)?;
        db.agent_operation(operation_id)
    })?;
    changed(app, kind, id);
    Ok((UndoOutcome { operation_id, restored_rows: 0, skipped_rows: 0, message }, record))
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|item| item.as_str().map(str::to_owned)).collect()
}

/// A material snapshot saved in an operation report.
fn material_from(value: &Value) -> Result<Material, String> {
    let versions = value["versions"].as_array().ok_or("操作记录里缺少素材版本")?;
    Ok(Material {
        id: value["id"].as_i64().unwrap_or_default(),
        title: value["title"].as_str().unwrap_or_default().to_owned(),
        text: value["text"].as_str().unwrap_or_default().to_owned(),
        tags: strings(&value["tags"]),
        updated_at: value["updatedAt"].as_str().unwrap_or_default().to_owned(),
        versions: versions.iter().map(|version| crate::db::materials::MaterialVersion {
            id: version["id"].as_i64().unwrap_or_default(),
            name: version["name"].as_str().unwrap_or_default().to_owned(),
            text: version["text"].as_str().unwrap_or_default().to_owned(),
            has_image: version["hasImage"].as_bool().unwrap_or_default(),
        }).collect(),
    })
}
