//! On-demand reads of individual library records and their original metadata.
use crate::app::AppRuntime;
use crate::db::agent::s;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Collection {
    Materials,
    PromptDocs,
    AutomationRules,
    ImportBatches,
}

#[derive(Deserialize)]
pub(super) struct ListArgs {
    collection: Collection,
    #[serde(default)]
    offset: u32,
}

pub(super) fn list(runtime: &AppRuntime, args: ListArgs) -> Result<Value, String> {
    let offset = args.offset as usize;
    let limit = 48;
    let (total, items): (usize, Vec<Value>) = match args.collection {
        Collection::Materials => runtime.agent_db(false, |db| {
            let page = db.list_materials("", &[], false, args.offset).map_err(s)?;
            Ok((page.total as usize, page.items.into_iter().map(|m|
                json!({"id":m.id,"title":m.title,"tags":m.tags,"versions":m.versions.len()})).collect()))
        })?,
        Collection::PromptDocs => {
            let docs = runtime.list_prompt_docs().map_err(s)?;
            (docs.len(), docs.into_iter().skip(offset).take(limit).map(|d|
                json!({"id":d.id,"title":d.title,"updatedAt":d.updated_at})).collect())
        }
        Collection::AutomationRules => runtime.agent_db(false, |db| {
            let rules = db.list_automation_rules().map_err(s)?;
            Ok((rules.len(), rules.into_iter().skip(offset).take(limit).map(|r|
                json!({"id":r.id,"name":r.name,"enabled":r.enabled})).collect()))
        })?,
        Collection::ImportBatches => runtime.agent_db(false, |db| {
            let batches = db.list_batches().map_err(s)?;
            Ok((batches.len(), batches.into_iter().skip(offset).take(limit).map(|b|
                json!({"id":b.id,"sourceType":b.source_type.as_str(),"sourcePath":b.source_path,
                    "importedAt":b.imported_at,"added":b.added_count,"skipped":b.skipped_count})).collect()))
        })?,
    };
    Ok(
        json!({"offset":offset,"nextOffset":next_offset(offset, items.len(), total),"total":total,"items":items}),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Item {
    Row,
    ImageMetadata,
    Material,
    PromptDoc,
    AutomationRule,
}

#[derive(Deserialize)]
pub(super) struct ReadArgs {
    kind: Item,
    id: Value,
    #[serde(default)]
    path: String,
    #[serde(default)]
    offset: usize,
}

pub(super) fn read(runtime: &AppRuntime, args: ReadArgs) -> Result<Value, String> {
    let id = || {
        args.id
            .as_i64()
            .or_else(|| args.id.as_str()?.parse().ok())
            .filter(|&id| id > 0)
            .ok_or_else(|| "需要有效的数字 ID".to_owned())
    };
    let item = match args.kind {
        Item::Row => runtime.agent_db(false, |db| {
            let row = db
                .get_rows_by_ids(&[id()?])
                .map_err(s)?
                .pop()
                .ok_or("图片 ID 不存在")?;
            let mut data = json!(row);
            if let Some(source) = row
                .artist_llm
                .and_then(|raw| serde_json::from_str(&raw).ok())
            {
                data["artistLlm"] = source;
            }
            Ok(data)
        })?,
        Item::ImageMetadata => {
            let directory = runtime.active_directory().map_err(s)?;
            let locator = directory
                .open_database()
                .map_err(s)?
                .row_image_locator(id()?)
                .map_err(s)?;
            let path = crate::storage::resolve_original_source(&directory, &locator).map_err(s)?;
            let read = crate::pipeline::read_metadata::read_novelai_metadata(&path, None);
            if read.is_unreadable() {
                return Err(read.warnings.join("；"));
            }
            let characters = crate::pipeline::character_prompts(&read.chunks);
            let mut chunks = serde_json::Map::new();
            for (key, text) in read.chunks {
                let parsed = (key == "Comment")
                    .then(|| serde_json::from_str(&text).ok())
                    .flatten();
                chunks.insert(key, parsed.unwrap_or(Value::String(text)));
            }
            json!({"chunks":chunks,"characters":characters,"warnings":read.warnings})
        }
        Item::Material => {
            runtime.agent_db(false, |db| Ok(json!(db.material(id()?).map_err(s)?)))?
        }
        Item::PromptDoc => json!(
            runtime
                .load_prompt_doc(args.id.as_str().ok_or("文档 ID 需要是字符串")?)
                .map_err(s)?
        ),
        Item::AutomationRule => runtime.agent_db(false, |db| {
            let id = id()?;
            let rule = db
                .list_automation_rules()
                .map_err(s)?
                .into_iter()
                .find(|r| r.id == id)
                .ok_or("自动规则 ID 不存在")?;
            Ok(json!(rule))
        })?,
    };
    let value = item
        .pointer(&args.path)
        .ok_or_else(|| format!("字段路径不存在：{}", args.path))?;
    Ok(value_page(value, args.offset))
}

fn next_offset(offset: usize, returned: usize, total: usize) -> Option<usize> {
    let end = offset.saturating_add(returned);
    (end < total).then_some(end)
}

fn preview(value: &Value) -> Value {
    match value {
        Value::String(text) if text.chars().take(201).count() > 200 => {
            json!({"type":"text","preview":text.chars().take(200).collect::<String>(),"truncated":true})
        }
        Value::Array(items) => json!({"type":"array","length":items.len()}),
        Value::Object(fields) => json!({"type":"object","fields":fields.len()}),
        scalar => scalar.clone(),
    }
}

fn value_page(value: &Value, offset: usize) -> Value {
    let (data, returned, total) = match value {
        Value::String(text) => {
            let page: String = text.chars().skip(offset).take(4096).collect();
            let returned = page.chars().count();
            (Value::String(page), returned, text.chars().count())
        }
        Value::Array(items) => {
            let page: Vec<_> = items.iter().skip(offset).take(40).map(preview).collect();
            let returned = page.len();
            (Value::Array(page), returned, items.len())
        }
        Value::Object(fields) => {
            let page: serde_json::Map<_, _> = fields
                .iter()
                .skip(offset)
                .take(40)
                .map(|(k, v)| (k.clone(), preview(v)))
                .collect();
            let returned = page.len();
            (Value::Object(page), returned, fields.len())
        }
        scalar => return json!({"value":scalar,"nextOffset":null}),
    };
    json!({"value":data,"offset":offset,"total":total,"nextOffset":next_offset(offset, returned, total)})
}
