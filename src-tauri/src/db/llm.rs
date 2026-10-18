//! Durable local extraction queue. Network calls never hold a database transaction.
use super::prompt_matching;
use super::style_extraction::{LlmSource, StyleChange, prompt_hash};
use super::{Database, DatabaseError, RowSelection};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const PROMPT_VERSION: &str = "style-extraction-v2";
pub const SETTINGS_KEY: &str = "local_llm_settings_v1";
pub fn invalid(message: impl Into<String>) -> DatabaseError {
    DatabaseError::IntegrityCheckFailed(message.into())
}
pub fn json<T: Serialize>(value: &T) -> Result<String, DatabaseError> {
    serde_json::to_string(value).map_err(|e| invalid(e.to_string()))
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, DatabaseError> {
    serde_json::from_str(value).map_err(|e| invalid(e.to_string()))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub concurrency: usize,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encrypted_key: Vec<u8>,
    #[serde(default)]
    pub has_key: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub auto_import: bool,
    pub auto_resume: bool,
    pub batch_size: usize,
    pub reasoning: String,
    pub channels: Vec<Channel>,
    pub ignored_tags: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_import: false,
            auto_resume: true,
            batch_size: 10,
            reasoning: "medium".into(),
            channels: vec![],
            ignored_tags: prompt_matching::default_ignored_tags(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub created_at: String,
    pub state: String,
    pub prompt_version: String,
    pub batch_size: usize,
    pub reasoning: String,
    pub channel_ids: Vec<String>,
    pub message: String,
    pub reuse: bool,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub applied: usize,
    pub empty: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub row_id: i64,
    pub artists: Option<String>,
    pub source: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub hash: String,
    pub prompt: String,
    pub status: String,
    pub artists: String,
    pub model: String,
    pub channel: String,
    pub error: String,
    pub attempts: usize,
    pub row_id: Option<i64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    pub rows: usize,
    pub skipped: usize,
    pub unique_prompts: usize,
    pub affected_rows: usize,
    pub reusable_prompts: usize,
    pub waiting_prompts: usize,
    pub request_prompts: usize,
}

fn valid_source(prompt: &str, artists: &Option<String>, text: &str) -> Option<LlmSource> {
    let source: LlmSource = serde_json::from_str(text).ok()?;
    (source.input_hash == prompt_hash(prompt)
        && prompt.contains(&source.artist_string)
        && artists.as_deref() == Some(source.artist_string.as_str()))
    .then_some(source)
}

impl Database {
    pub fn llm_id(&self) -> Result<String, DatabaseError> {
        self.setting("llm_library_id")?
            .ok_or_else(|| invalid("资料库缺少标识"))
    }
    pub fn llm_settings(&self) -> Result<Settings, DatabaseError> {
        let mut settings: Settings = self.setting(SETTINGS_KEY)?
            .map(|x| decode(&x))
            .unwrap_or_else(|| Ok(Settings::default()))?;
        settings.ignored_tags = prompt_matching::clean_ignored_tags(&settings.ignored_tags);
        Ok(settings)
    }
    fn llm_candidates(
        &mut self,
        selection: Option<&RowSelection>,
        include: bool,
        all_matching: bool,
    ) -> Result<(Estimate, HashMap<String, Vec<Target>>), DatabaseError> {
        let selected = selection
            .map(|s| self.selected_row_ids(s))
            .transpose()
            .map_err(|e| invalid(e.to_string()))?
            .map(|ids| ids.into_iter().collect::<HashSet<_>>());
        let mut stmt = self.connection.prepare(
            "SELECT id, COALESCE(positive_prompt,''), artists, artist_llm FROM rows ORDER BY id",
        )?;
        let values = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stats = Estimate {
            rows: 0,
            skipped: 0,
            unique_prompts: 0,
            affected_rows: 0,
            reusable_prompts: 0,
            waiting_prompts: 0,
            request_prompts: 0,
        };
        let ignored = self.llm_settings()?.ignored_tags;
        let mut grouped: HashMap<String, Vec<Target>> = HashMap::new();
        for (id, prompt, artists, source) in &values {
            if selected.as_ref().is_some_and(|s| !s.contains(id)) {
                continue;
            }
            stats.rows += 1;
            if prompt.trim().is_empty()
                || (!include
                    && source
                        .as_deref()
                        .and_then(|s| valid_source(prompt, artists, s))
                        .is_some())
            {
                stats.skipped += 1;
                continue;
            }
            grouped.entry(prompt.clone()).or_default().push(Target {
                row_id: *id,
                artists: artists.clone(),
                source: source.clone(),
            });
        }
        if all_matching {
            let keys = grouped
                .keys()
                .map(|prompt| prompt_matching::key(prompt, &ignored))
                .collect::<HashSet<_>>();
            for targets in grouped.values_mut() {
                targets.clear();
            }
            for (id, prompt, artists, source) in values {
                if !include
                    && source
                        .as_deref()
                        .and_then(|s| valid_source(&prompt, &artists, s))
                        .is_some()
                {
                    continue;
                }
                if keys.contains(&prompt_matching::key(&prompt, &ignored)) {
                    let targets = grouped.entry(prompt).or_default();
                    targets.push(Target {
                        row_id: id,
                        artists,
                        source,
                    });
                }
            }
        }
        stats.unique_prompts = grouped
            .keys()
            .map(|prompt| prompt_matching::key(prompt, &ignored))
            .collect::<HashSet<_>>()
            .len();
        stats.affected_rows = grouped.values().map(Vec::len).sum();
        Ok((stats, grouped))
    }
    pub fn llm_estimate(
        &mut self,
        selection: Option<&RowSelection>,
        include: bool,
        all_matching: bool,
    ) -> Result<Estimate, DatabaseError> {
        let (mut stats, groups) = self.llm_candidates(selection, include, all_matching)?;
        let ignored = self.llm_settings()?.ignored_tags;
        self.llm_refresh_match_keys(&ignored)?;
        let waiting = {
            let mut stmt = self.connection.prepare("SELECT DISTINCT i.match_key FROM llm_items i JOIN llm_jobs j ON j.id=i.job_id WHERE i.status='pending' AND j.state IN ('queued','running')")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?
        };
        let mut seen = HashSet::new();
        for prompt in groups.keys() {
            let key = prompt_matching::key(prompt, &ignored);
            if !seen.insert(key.clone()) {
                continue;
            }
            if !include
                && self
                    .llm_cached_prompt(prompt, &key, &ignored, None)?
                    .is_some()
            {
                stats.reusable_prompts += 1;
            } else if !include && waiting.contains(&key) {
                stats.waiting_prompts += 1;
            } else {
                stats.request_prompts += 1;
            }
        }
        Ok(stats)
    }
    pub fn llm_create(
        &mut self,
        selection: Option<&RowSelection>,
        include: bool,
        all_matching: bool,
        channel_ids: Vec<String>,
        automatic: bool,
    ) -> Result<Option<String>, DatabaseError> {
        let settings = self.llm_settings()?;
        if automatic && !settings.auto_import {
            return Ok(None);
        }
        let (_, mut groups) = self.llm_candidates(selection, include, all_matching)?;
        // The same row can be imported/queued again while another request is still running.
        let mut queued = HashSet::new();
        let mut stmt = self.connection.prepare("SELECT i.hash,i.targets FROM llm_items i JOIN llm_jobs j ON j.id=i.job_id WHERE i.status='pending' AND j.state NOT IN ('cancelled','undone')")?;
        for value in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (hash, text) = value?;
            for target in decode::<Vec<Target>>(&text)? {
                queued.insert((target.row_id, hash.clone()));
            }
        }
        drop(stmt);
        groups.retain(|prompt, targets| {
            let hash = prompt_hash(prompt);
            targets.retain(|t| !queued.contains(&(t.row_id, hash.clone())));
            !targets.is_empty()
        });
        if groups.is_empty() {
            return Ok(None);
        }
        let channels = if automatic {
            settings
                .channels
                .iter()
                .filter(|c| c.enabled)
                .map(|c| c.id.clone())
                .collect()
        } else {
            channel_ids
        };
        let id = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO llm_jobs(id,created_at,state,prompt_version,batch_size,reasoning,channel_ids,reuse,total) VALUES (?1,?2,'queued',?3,?4,?5,?6,?7,?8)", params![id, now(), PROMPT_VERSION, settings.batch_size as i64, settings.reasoning, json(&channels)?, !include,groups.len() as i64])?;
        for (prompt, targets) in groups {
            tx.execute(
                "INSERT INTO llm_items(job_id,hash,prompt,targets,match_key) VALUES (?1,?2,?3,?4,?5)",
                params![id, prompt_hash(&prompt), prompt, json(&targets)?, prompt_matching::key(&prompt, &settings.ignored_tags)],
            )?;
        }
        tx.commit()?;
        Ok(Some(id))
    }
    pub fn llm_jobs(&self) -> Result<Vec<Job>, DatabaseError> {
        let mut stmt = self.connection.prepare("SELECT id,created_at,state,prompt_version,batch_size,reasoning,channel_ids,message,total,done,failed,applied,empty,reuse FROM llm_jobs ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Job {
                id: r.get(0)?,
                created_at: r.get(1)?,
                state: r.get(2)?,
                prompt_version: r.get(3)?,
                batch_size: r.get::<_, i64>(4)? as usize,
                reasoning: r.get(5)?,
                channel_ids: serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default(),
                message: r.get(7)?,
                total: r.get::<_, i64>(8)? as usize,
                done: r.get::<_, i64>(9)? as usize,
                failed: r.get::<_, i64>(10)? as usize,
                applied: r.get::<_, i64>(11)? as usize,
                empty: r.get::<_, i64>(12)? as usize,
                reuse: r.get(13)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub fn llm_items(
        &self,
        job: &str,
        status: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<Item>, DatabaseError> {
        let mut stmt = self.connection.prepare("SELECT hash,prompt,status,artists,model,channel,error,attempts,json_extract(targets,'$[0].rowId') FROM llm_items WHERE job_id=?1 AND (?2 IS NULL OR status=?2) ORDER BY rowid LIMIT ?3 OFFSET ?4")?;
        Ok(stmt
            .query_map(
                params![
                    job,
                    status,
                    limit.min(i64::MAX as usize) as i64,
                    offset.min(i64::MAX as usize) as i64
                ],
                |r| {
                    Ok(Item {
                        hash: r.get(0)?,
                        prompt: r.get(1)?,
                        status: r.get(2)?,
                        artists: r.get(3)?,
                        model: r.get(4)?,
                        channel: r.get(5)?,
                        error: r.get(6)?,
                        attempts: r.get::<_, i64>(7)? as usize,
                        row_id: r.get(8)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn llm_state(
        &mut self,
        job: &str,
        state: &str,
        message: &str,
    ) -> Result<(), DatabaseError> {
        self.connection.execute(
            "UPDATE llm_jobs SET state=?2,message=?3 WHERE id=?1",
            params![job, state, message],
        )?;
        Ok(())
    }
    pub fn llm_claim(&mut self, job: &str) -> Result<bool, DatabaseError> {
        Ok(self.connection.execute("UPDATE llm_jobs SET state='running',message='' WHERE id=?1 AND state IN ('queued','running')",[job])?>0)
    }
    pub fn llm_worker_state(
        &mut self,
        job: &str,
        state: &str,
        message: &str,
    ) -> Result<(), DatabaseError> {
        self.connection.execute(
            "UPDATE llm_jobs SET state=?2,message=?3 WHERE id=?1 AND state IN ('queued','running')",
            params![job, state, message],
        )?;
        Ok(())
    }
    pub fn llm_recover(&mut self) -> Result<(), DatabaseError> {
        let state = if self.llm_settings()?.auto_resume {
            "queued"
        } else {
            "paused"
        };
        self.connection.execute(
            "UPDATE llm_jobs SET state=?1 WHERE state IN ('queued','running')",
            [state],
        )?;
        Ok(())
    }
    /// Indexed lookups include imported historical results. Rule changes rebuild derived keys only.
    pub fn llm_refresh_match_keys(&mut self, ignored: &[String]) -> Result<(), DatabaseError> {
        let rules = prompt_matching::rules_key(ignored);
        let rebuild = self.setting("llm_match_rules")?.as_deref() != Some(&rules);
        if !rebuild {
            let pending: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM llm_items WHERE match_key IS NULL) OR EXISTS(SELECT 1 FROM rows WHERE artist_llm IS NOT NULL AND llm_match_key IS NULL)", [], |r| r.get(0))?;
            if !pending {
                return Ok(());
            }
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if rebuild {
            tx.execute_batch("UPDATE llm_items SET match_key=NULL; UPDATE rows SET llm_match_key=NULL WHERE llm_match_key IS NOT NULL;")?;
        }
        let items = {
            let mut stmt =
                tx.prepare("SELECT job_id,hash,prompt FROM llm_items WHERE match_key IS NULL")?;
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        {
            let mut update =
                tx.prepare("UPDATE llm_items SET match_key=?3 WHERE job_id=?1 AND hash=?2")?;
            for (job, hash, prompt) in items {
                update.execute(params![job, hash, prompt_matching::key(&prompt, ignored)])?;
            }
        }
        let rows = {
            let mut stmt = tx.prepare("SELECT id,COALESCE(positive_prompt,'') FROM rows WHERE artist_llm IS NOT NULL AND llm_match_key IS NULL")?;
            stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        {
            let mut update = tx.prepare("UPDATE rows SET llm_match_key=?2 WHERE id=?1")?;
            for (id, prompt) in rows {
                update.execute(params![id, prompt_matching::key(&prompt, ignored)])?;
            }
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('llm_match_rules',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[rules])?;
        tx.commit()?;
        Ok(())
    }
    pub fn llm_cached(
        &mut self,
        item: &Item,
        only_job: Option<&str>,
    ) -> Result<Option<(String, String, String)>, DatabaseError> {
        let ignored = self.llm_settings()?.ignored_tags;
        self.llm_refresh_match_keys(&ignored)?;
        self.llm_cached_prompt(
            &item.prompt,
            &prompt_matching::key(&item.prompt, &ignored),
            &ignored,
            only_job,
        )
    }
    fn llm_cached_prompt(
        &self,
        prompt: &str,
        key: &str,
        ignored: &[String],
        only_job: Option<&str>,
    ) -> Result<Option<(String, String, String)>, DatabaseError> {
        let mut stmt = self.connection.prepare("SELECT i.prompt,i.artists,i.model,i.channel,i.status FROM llm_items i JOIN llm_jobs j ON j.id=i.job_id WHERE i.match_key=?1 AND i.status IN ('ok','none') AND (?3 IS NULL OR i.job_id=?3) ORDER BY (i.prompt=?2) DESC,j.created_at DESC")?;
        let mut results = stmt.query(params![key, prompt, only_job])?;
        while let Some(row) = results.next()? {
            let original: String = row.get(0)?;
            let artists: String = row.get(1)?;
            let status: String = row.get(4)?;
            if (status == "none") != artists.is_empty() {
                continue;
            }
            if let Some(projected) = prompt_matching::project(&original, &artists, prompt, ignored) {
                return Ok(Some((projected, row.get(2)?, row.get(3)?)));
            }
        }
        // Forced work may reuse a fresh result from its own task, never an older result.
        if only_job.is_some() {
            return Ok(None);
        }
        let mut stmt = self.connection.prepare("SELECT positive_prompt,artists,artist_llm FROM rows WHERE artist_llm IS NOT NULL AND llm_match_key=?1 ORDER BY (positive_prompt=?2) DESC,id DESC")?;
        let mut results = stmt.query(params![key, prompt])?;
        while let Some(row) = results.next()? {
            let original: String = row.get(0)?;
            let artists: Option<String> = row.get(1)?;
            let text: String = row.get(2)?;
            if let Some(source) = valid_source(&original, &artists, &text)
                && let Some(projected) =
                    prompt_matching::project(&original, &source.artist_string, prompt, ignored)
            {
                return Ok(Some((projected, source.model, "资料库已有结果".into())));
            }
        }
        Ok(None)
    }
    pub fn llm_save_result(
        &mut self,
        job: &str,
        item: &Item,
        artists: &str,
        model: &str,
        channel: &str,
    ) -> Result<usize, DatabaseError> {
        if !item.prompt.contains(artists) {
            return Err(invalid("提取结果不是连续原文"));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let saved: Option<(String,String,String)> = tx.query_row("SELECT i.targets,i.status,j.state FROM llm_items i JOIN llm_jobs j ON j.id=i.job_id WHERE i.job_id=?1 AND i.hash=?2 AND i.prompt=?3",params![job,item.hash,item.prompt], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((targets, status, state)) = saved else {
            return Ok(0);
        };
        if status != "pending" || !["queued", "running"].contains(&state.as_str()) {
            return Ok(0);
        }
        let source = json(&LlmSource {
            input_hash: item.hash.clone(),
            artist_string: artists.into(),
            model: model.into(),
            prompt_version: PROMPT_VERSION.into(),
            processed_at: now(),
        })?;
        let targets: Vec<Target> = decode(&targets)?;
        let mut changes = Vec::new();
        let mut conflicts = 0;
        for target in targets {
            let changed = tx.execute("UPDATE rows SET artists=?2,artist_llm=?3 WHERE id=?1 AND positive_prompt=?4 AND artists IS ?5 AND artist_llm IS ?6",params![target.row_id,artists,source,item.prompt,target.artists,target.source])?;
            if changed > 0 {
                changes.push(StyleChange {
                    row_id: target.row_id,
                    input_hash: item.hash.clone(),
                    old_artists: target.artists,
                    old_source: target.source,
                    new_artists: Some(artists.into()),
                    new_source: Some(source.clone()),
                });
            } else {
                conflicts += 1;
            }
        }
        let error = if conflicts > 0 {
            format!("{conflicts} 张图片已修改或删除，未覆盖")
        } else {
            String::new()
        };
        tx.execute("UPDATE llm_items SET status=?3,artists=?4,model=?5,channel=?6,error=?7,changes=?8 WHERE job_id=?1 AND hash=?2", params![job,item.hash,if artists.is_empty(){"none"}else{"ok"},artists,model,channel,error,json(&changes)?])?;
        tx.execute(
            "UPDATE llm_jobs SET done=done+1,empty=empty+?2,applied=applied+?3 WHERE id=?1",
            params![job, artists.is_empty() as i64, changes.len() as i64],
        )?;
        tx.commit()?;
        self.bump_data_version();
        Ok(changes.len())
    }
    pub fn llm_fail(&mut self, job: &str, item: &Item, message: &str) -> Result<(), DatabaseError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed=tx.execute("UPDATE llm_items SET attempts=attempts+1,error=?3,status=CASE WHEN attempts+1>=3 THEN 'error' ELSE 'pending' END WHERE job_id=?1 AND hash=?2 AND status='pending' AND EXISTS(SELECT 1 FROM llm_jobs WHERE id=?1 AND state IN ('queued','running'))",params![job,item.hash,message])?;
        if changed > 0 {
            tx.execute("UPDATE llm_jobs SET failed=failed+1 WHERE id=?1 AND EXISTS(SELECT 1 FROM llm_items WHERE job_id=?1 AND hash=?2 AND status='error')",params![job,item.hash])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn llm_action(
        &mut self,
        job: &str,
        action: &str,
        channels: Vec<String>,
    ) -> Result<(), DatabaseError> {
        let state: String =
            self.connection
                .query_row("SELECT state FROM llm_jobs WHERE id=?1", [job], |r| {
                    r.get(0)
                })?;
        match action {
            "channels" => {
                self.connection.execute(
                    "UPDATE llm_jobs SET channel_ids=?2 WHERE id=?1",
                    params![job, json(&channels)?],
                )?;
            }
            "pause" => {
                self.llm_state(job, "paused", "已手动暂停")?;
            }
            "cancel" => {
                self.llm_state(job, "cancelled", "已结束，保留完成结果")?;
            }
            "resume" | "retry" => {
                if state == "undone" {
                    return Err(invalid("已撤销任务不能继续，请新建任务"));
                }
                let version: String = self.connection.query_row(
                    "SELECT prompt_version FROM llm_jobs WHERE id=?1",
                    [job],
                    |r| r.get(0),
                )?;
                if version != PROMPT_VERSION {
                    return Err(invalid("提取规则已变化，请创建新任务"));
                }
                let tx = self
                    .connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)?;
                if action == "retry" {
                    tx.execute("UPDATE llm_items SET status='pending',attempts=0,error='' WHERE job_id=?1 AND status='error'",[job])?;
                    tx.execute("UPDATE llm_jobs SET failed=0 WHERE id=?1", [job])?;
                }
                tx.execute(
                    "UPDATE llm_jobs SET state='queued',message='',channel_ids=?2 WHERE id=?1",
                    params![job, json(&channels)?],
                )?;
                tx.commit()?;
            }
            "undo" => {
                if ["queued", "running"].contains(&state.as_str()) {
                    return Err(invalid("请先暂停任务再撤销"));
                }
                let mut stmt = self
                    .connection
                    .prepare("SELECT changes FROM llm_items WHERE job_id=?1")?;
                let texts = stmt
                    .query_map([job], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                drop(stmt);
                let tx = self
                    .connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)?;
                let mut conflicts = 0;
                for text in texts {
                    for change in decode::<Vec<StyleChange>>(&text)? {
                        let current: Option<String> = tx
                            .query_row(
                                "SELECT positive_prompt FROM rows WHERE id=?1",
                                [change.row_id],
                                |r| r.get(0),
                            )
                            .optional()?
                            .flatten();
                        if current.as_deref().map(prompt_hash).as_deref()
                            != Some(&change.input_hash)
                        {
                            conflicts += 1;
                            continue;
                        }
                        if tx.execute("UPDATE rows SET artists=?2,artist_llm=?3 WHERE id=?1 AND artists IS ?4 AND artist_llm IS ?5",params![change.row_id,change.old_artists,change.old_source,change.new_artists,change.new_source])?==0 { conflicts+=1; }
                    }
                }
                tx.execute(
                    "UPDATE llm_jobs SET state='undone',message=?2 WHERE id=?1",
                    params![job, format!("已撤销；{conflicts} 张后续发生变化，保留现状")],
                )?;
                tx.commit()?;
                self.bump_data_version();
            }
            _ => return Err(invalid("未知任务操作")),
        }
        Ok(())
    }
}
