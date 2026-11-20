//! Read-only SQL over the library tables, so the model can ask what the fixed tools cannot.
//!
//! Queries run on a separate read-only connection (outside the app's database lock) guarded by an
//! authorizer: only SELECTs over the library tables below, no pragmas, attachments or writes.
//! Changes still go through preview → apply → undo; a query only finds rows (see `expand_target`).
use std::collections::HashSet;
use std::time::{Duration, Instant};

use regex::RegexBuilder;
use rusqlite::functions::FunctionFlags;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppRuntime;
use crate::automation::text::prompt_token_set;
use crate::pipeline::prompt_text::normalize_prompt_token;

/// Tables a query may read. Agent conversations, settings and caches are not among them.
const TABLES: &[&str] = &[
    "rows", "tags", "row_tags", "groups", "import_batches", "automation_rules", "dedupe_aliases",
    "artist_representatives", "materials", "material_versions", "material_tags",
];
/// Built-in table-valued functions a query may use.
const TABLE_FUNCTIONS: &[&str] = &["json_each", "json_tree"];
/// Image bytes read as NULL instead of failing the query.
const HIDDEN_COLUMNS: &[(&str, &str)] = &[
    ("materials", "cover"), ("materials", "thumbnail"), ("material_versions", "cover"), ("material_versions", "thumbnail"),
];
const BLOCKED_FUNCTIONS: &[&str] = &["load_extension", "zeroblob", "randomblob", "readfile", "writefile", "edit", "fts3_tokenizer"];
const TIME_LIMIT: Duration = Duration::from_secs(15);
const DEFAULT_ROWS: usize = 100;
const MAX_ROWS: usize = 1000;
/// Characters of result text returned to the model; rows beyond this are counted but not sent.
const OUTPUT_CHARS: usize = 60_000;
const CELL_CHARS: usize = 4_000;
/// Most image IDs a `target.sql` may select.
const MAX_TARGET_IDS: usize = 200_000;

pub const GUIDE: &str = r#"- query_library_sql 执行一条只读 SQL（SQLite 语法，只能 SELECT / WITH），用来做现有工具做不到的查询和统计：多条件组合、按命中几个 TAG 筛图、跨表统计、找异常数据等。改数据仍然只能走预演 → 执行 → 撤销。
- 查到的图片可以直接交给其它工具：在任何 target 里写 sql（第一列是图片 id），配合 scope=all 表示「就是这些图」，配合 scope=rowIds 表示和 rowIds 取交集；其它 target 字段照常叠加。不用把 id 抄成长列表。
- 先想清楚再查：能用 COUNT / GROUP BY 聚合就不要把大量行拉回来；只选需要的列，长提示词用 substr() 截短。默认返回 100 行，limit 最多 1000；超过 15 秒的查询会被中止。
- 表（具体列见 schema）：
  - rows：每张图一行。id=图片编号（就是其它工具里的 rowId）；positive_prompt / character_prompt / negative_prompt 是正向 / 角色（多角色合在一起）/ 负面提示词；artists=画师串，artist_match_key=画师串去重分组键（同一个画师串的图这个值相同）；note 备注；group_id→groups.id（NULL=未分组）；favorite 0/1；batch_id→import_batches.id；generation_model 模型原名、generation_sampler、generation_steps、generation_seed（文本）、generation_scale（CFG）、generation_cfg_rescale、generation_noise_schedule；image_width / image_height；vibe_reference_count；image_path / image_folder 原文件位置；metadata_failed=1 表示没读到生成信息；updated_at 最后修改时间；time 是旧数据自带的时间文本，可能为空。
  - tags(id, name) 与 row_tags(row_id, tag_id)：图片 TAG（TAG 名区分大小写）。groups(id, name)：分组。
  - import_batches：导入批次，imported_at 是导入时间。automation_rules：自动规则，conditions_json / actions_json 是 JSON 文本。
  - dedupe_aliases(mode, key, alias)：mode='artists' 时 key 对应 rows.artist_match_key，alias 是画师串别名。artist_representatives(artist_key, row_id)：画师串代表图。
  - materials / material_versions / material_tags：素材、素材版本正文和素材 TAG（封面图片不可读）。
- 时间列都是 UTC 文本（如 2026-11-20T03:12:00.000Z）。按本地时间比较或显示用 datetime(列, 'localtime')，例如 date(updated_at, 'localtime') = '2026-11-20'。
- 提示词是逗号分隔的 TAG，可能带权重写法（1.2::tag::、{tag}、[tag]、(tag:1.1)）。不要用 LIKE 硬匹配 TAG，用这几个函数（忽略大小写和权重写法，下划线等同空格）：
  - has_tag(文本, 'blue eyes')：这段提示词里有没有这个 TAG，返回 1/0。
  - tag_hits(文本, 'a, b, c, d')：列出的 TAG 里命中了几个。例：tag_hits(positive_prompt, 'a, b, c, d') >= 3。
  - prompt_tags(文本)：整理后的 TAG 列表（JSON 数组），配合 json_each 做词频，例：SELECT t.value, COUNT(*) FROM rows, json_each(prompt_tags(rows.positive_prompt)) t GROUP BY t.value ORDER BY 2 DESC LIMIT 30。
  - 正则：文本 REGEXP '模式'（Rust regex 语法，区分大小写，不区分时在模式前加 (?i)）。
- 例：没有 TAG「花绘」、且正向里 4 个特征至少中 3 个的图：SELECT id FROM rows WHERE tag_hits(positive_prompt, 'a, b, c, d') >= 3 AND id NOT IN (SELECT row_id FROM row_tags JOIN tags ON tags.id = row_tags.tag_id WHERE tags.name = '花绘')"#;

fn table_allowed(table: &str) -> bool {
    TABLES.iter().chain(TABLE_FUNCTIONS).any(|name| name.eq_ignore_ascii_case(table))
}

fn authorize(context: AuthContext<'_>) -> Authorization {
    match context.action {
        AuthAction::Select | AuthAction::Recursive => Authorization::Allow,
        AuthAction::Read { table_name, column_name } => {
            if HIDDEN_COLUMNS.iter().any(|(table, column)| table.eq_ignore_ascii_case(table_name) && column.eq_ignore_ascii_case(column_name)) {
                Authorization::Ignore
            } else if context.database_name.is_none_or(|name| name == "main") && table_allowed(table_name) {
                Authorization::Allow
            } else {
                Authorization::Deny
            }
        }
        AuthAction::Function { function_name } => {
            if BLOCKED_FUNCTIONS.iter().any(|name| name.eq_ignore_ascii_case(function_name)) { Authorization::Deny } else { Authorization::Allow }
        }
        _ => Authorization::Deny,
    }
}

fn tag_key(text: &str) -> String {
    normalize_prompt_token(text).replace('_', " ")
}

fn tag_set(text: &str) -> HashSet<String> {
    prompt_token_set(text).into_iter().map(|tag| tag.replace('_', " ")).collect()
}

fn text_arg(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => number.to_string(),
        _ => String::new(),
    }
}

fn register(connection: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    connection.create_scalar_function("has_tag", 2, flags, |ctx| {
        let text = text_arg(ctx.get_raw(0));
        let tag = ctx.get_or_create_aux(1, |value| Ok::<_, rusqlite::Error>(tag_key(&text_arg(value))))?;
        Ok(i64::from(!tag.is_empty() && tag_set(&text).contains(tag.as_str())))
    })?;
    connection.create_scalar_function("tag_hits", 2, flags, |ctx| {
        let text = text_arg(ctx.get_raw(0));
        let wanted = ctx.get_or_create_aux(1, |value| Ok::<_, rusqlite::Error>(tag_set(&text_arg(value))))?;
        let present = tag_set(&text);
        Ok(wanted.iter().filter(|tag| present.contains(*tag)).count() as i64)
    })?;
    connection.create_scalar_function("prompt_tags", 1, flags, |ctx| {
        let text = text_arg(ctx.get_raw(0));
        let mut seen = HashSet::new();
        let tags = text
            .split([',', '，', '\n', '\r'])
            .map(tag_key)
            .filter(|tag| !tag.is_empty() && seen.insert(tag.clone()))
            .collect::<Vec<_>>();
        Ok(Value::from(tags).to_string())
    })?;
    connection.create_scalar_function("regexp", 2, flags, |ctx| {
        let pattern = ctx.get_or_create_aux(0, |value| {
            RegexBuilder::new(&text_arg(value)).size_limit(1 << 20).build().map_err(|error| format!("正则写法不对：{error}"))
        })?;
        let text = text_arg(ctx.get_raw(1));
        Ok(i64::from(pattern.is_match(&text)))
    })?;
    Ok(())
}

fn open(runtime: &AppRuntime) -> Result<Connection, String> {
    let path = runtime.agent_db(false, |db| db.path().ok_or_else(|| "找不到资料库文件".to_owned()))?;
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|error| format!("打开资料库失败：{error}"))?;
    connection.busy_timeout(Duration::from_secs(5)).map_err(|error| error.to_string())?;
    connection.execute_batch("PRAGMA query_only = 1;").map_err(|error| error.to_string())?;
    register(&connection).map_err(|error| error.to_string())?;
    Ok(connection)
}

/// A read-only connection with the authorizer and a time limit installed.
fn guarded(runtime: &AppRuntime) -> Result<Connection, String> {
    let connection = open(runtime)?;
    connection.authorizer(Some(authorize)).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + TIME_LIMIT;
    connection.progress_handler(10_000, Some(move || Instant::now() > deadline)).map_err(|error| error.to_string())?;
    Ok(connection)
}

fn explain(error: rusqlite::Error) -> String {
    let text = error.to_string();
    if matches!(error, rusqlite::Error::MultipleStatement) {
        return "一次只能执行一条 SQL 语句".into();
    }
    if text.contains("interrupted") {
        return format!("查询超过 {} 秒，已中止。请缩小范围、少用相关子查询，或先聚合再查。", TIME_LIMIT.as_secs());
    }
    if text.contains("not authorized") || text.contains("prohibited") {
        return format!("这条 SQL 用到了不允许的内容：只能 SELECT，且只能读这些表：{}。（{text}）", TABLES.join("、"));
    }
    format!("SQL 出错：{text}")
}

fn prepare<'c>(connection: &'c Connection, sql: &str) -> Result<rusqlite::Statement<'c>, String> {
    let sql = sql.trim().trim_end_matches(';');
    if sql.is_empty() {
        return Err("SQL 是空的".into());
    }
    let statement = connection.prepare(sql).map_err(explain)?;
    if !statement.readonly() || statement.column_count() == 0 {
        return Err("只能执行只读的查询语句（SELECT / WITH）".into());
    }
    Ok(statement)
}

fn cell(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => json!(number),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            let count = text.chars().count();
            if count > CELL_CHARS {
                json!(format!("{}…（共 {count} 字，已截断；需要全文用 substr() 分段取）", text.chars().take(CELL_CHARS).collect::<String>()))
            } else {
                json!(text)
            }
        }
        ValueRef::Blob(bytes) => json!(format!("（二进制数据 {} 字节）", bytes.len())),
    }
}

#[derive(Deserialize)]
pub struct QueryArgs {
    sql: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// Run one query; rows beyond the limit are counted, not returned.
pub fn query(runtime: &AppRuntime, args: QueryArgs) -> Result<Value, String> {
    let connection = guarded(runtime)?;
    let mut statement = prepare(&connection, &args.sql)?;
    let columns = statement.column_names().into_iter().map(str::to_owned).collect::<Vec<_>>();
    let limit = args.limit.unwrap_or(DEFAULT_ROWS).clamp(1, MAX_ROWS);
    let started = Instant::now();
    let mut rows = statement.raw_query();
    let mut returned = Vec::new();
    let (mut total, mut chars, mut cut_by_size) = (0usize, 0usize, false);
    while let Some(row) = rows.next().map_err(explain)? {
        total += 1;
        if returned.len() >= limit || cut_by_size {
            continue;
        }
        let values = (0..columns.len()).map(|index| row.get_ref(index).map(cell)).collect::<Result<Vec<_>, _>>().map_err(explain)?;
        let size = Value::from(values.clone()).to_string().chars().count();
        if chars + size > OUTPUT_CHARS && !returned.is_empty() {
            cut_by_size = true;
            continue;
        }
        chars += size;
        returned.push(Value::from(values));
    }
    let mut output = json!({
        "columns": columns,
        "rows": returned,
        "returned": returned.len(),
        "totalRows": total,
        "milliseconds": started.elapsed().as_millis() as u64,
    });
    if returned.len() < total {
        output["note"] = json!(if cut_by_size {
            format!("结果文字太多，只返回了前 {} 行（共 {total} 行）。请只选需要的列、用 substr() 截短长文本，或改用聚合。", returned.len())
        } else {
            format!("只返回了前 {} 行（共 {total} 行）。需要更多可调大 limit（最多 {MAX_ROWS}），或改用聚合 / 分页（LIMIT … OFFSET …）。", returned.len())
        });
    }
    Ok(output)
}

/// Image IDs from the first column of a query, in order, without duplicates.
fn query_ids(runtime: &AppRuntime, sql: &str) -> Result<Vec<i64>, String> {
    let connection = guarded(runtime)?;
    let mut statement = prepare(&connection, sql)?;
    let mut rows = statement.raw_query();
    let (mut ids, mut seen) = (Vec::new(), HashSet::new());
    while let Some(row) = rows.next().map_err(explain)? {
        let id = match row.get_ref(0).map_err(explain)? {
            ValueRef::Integer(id) => id,
            ValueRef::Text(text) => std::str::from_utf8(text).ok().and_then(|text| text.trim().parse().ok())
                .ok_or("target.sql 的第一列必须是图片 id（rows.id）")?,
            ValueRef::Null => continue,
            _ => return Err("target.sql 的第一列必须是图片 id（rows.id）".into()),
        };
        if seen.insert(id) {
            ids.push(id);
            if ids.len() > MAX_TARGET_IDS {
                return Err(format!("target.sql 查出的图片超过 {MAX_TARGET_IDS} 张，请缩小范围"));
            }
        }
    }
    Ok(ids)
}

/// Replace `target.sql` with the image IDs it selects, so every tool taking a target can use it.
pub fn expand_target(runtime: &AppRuntime, arguments: &Value) -> Result<Option<Value>, String> {
    let Some(sql) = arguments["target"].get("sql") else { return Ok(None) };
    let sql = sql.as_str().ok_or("target.sql 必须是一条 SQL 文本")?.to_owned();
    let mut arguments = arguments.clone();
    let target = arguments["target"].as_object_mut().ok_or("target 格式不对")?;
    target.remove("sql");
    let ids = query_ids(runtime, &sql)?;
    match target.get("scope").and_then(Value::as_str) {
        Some("all") => {
            target.insert("scope".into(), json!("rowIds"));
            target.insert("rowIds".into(), json!(ids));
        }
        Some("rowIds") => {
            let found = ids.into_iter().collect::<HashSet<_>>();
            let kept = target.get("rowIds").and_then(Value::as_array).into_iter().flatten()
                .filter_map(Value::as_i64).filter(|id| found.contains(id)).collect::<Vec<_>>();
            target.insert("rowIds".into(), json!(kept));
        }
        _ => return Err("target.sql 只能和 scope=all（就用查出来的图）或 scope=rowIds（取交集）一起用；要限定在主窗口选中或筛选范围内，先用 get_main_window_state 或 find_rows 拿到 id。".into()),
    }
    if target["rowIds"].as_array().is_some_and(Vec::is_empty) {
        return Err("target.sql 没有查到任何图片，请先用 query_library_sql 检查查询结果".into());
    }
    Ok(Some(arguments))
}

/// Columns of every readable table, read from the open library so they never go stale.
pub fn schema(runtime: &AppRuntime) -> Result<Value, String> {
    let connection = open(runtime)?;
    let mut tables = serde_json::Map::new();
    for table in TABLES {
        let mut statement = connection.prepare(&format!("SELECT name, type FROM pragma_table_info('{table}')")).map_err(|error| error.to_string())?;
        let columns = statement
            .query_map([], |row| Ok(format!("{} {}", row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|column| !HIDDEN_COLUMNS.iter().any(|(t, c)| t == table && column.split(' ').next() == Some(*c)))
            .collect::<Vec<_>>();
        if !columns.is_empty() {
            tables.insert((*table).to_owned(), json!(columns.join(", ")));
        }
    }
    Ok(Value::Object(tables))
}
