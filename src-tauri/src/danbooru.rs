use std::thread;
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::db::{ArtistDictionaryInput, DanbooruArtistRecord, DanbooruArtistTag, DanbooruTagAlias};

const API_ROOT: &str = "https://danbooru.donmai.us";
const PAGE_SIZE: usize = 1_000;
const MAX_PAGES_PER_RESOURCE: usize = 2_000;
const USER_AGENT: &str = concat!("Smart-Spreadsheet/", env!("CARGO_PKG_VERSION"));

const TAGS_QUERY: &str = "tags.json?search%5Bcategory%5D=1&search%5Border%5D=id&limit=1000&only=id,name,post_count,category,is_deprecated";
const ARTISTS_QUERY: &str = "artists.json?search%5Border%5D=id&search%5Bis_deleted%5D=false&search%5Bany_other_name_like%5D=%2A&limit=1000&only=id,name,other_names,is_deleted,is_banned";
const ALIASES_QUERY: &str = "tag_aliases.json?search%5Border%5D=id&search%5Bconsequent_tag%5D%5Bcategory%5D=1&limit=1000&only=id,antecedent_name,consequent_name,status";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtistDictionarySyncStage {
    Tags,
    Artists,
    Aliases,
    Saving,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistDictionarySyncProgress {
    pub stage: ArtistDictionarySyncStage,
    pub pages_fetched: usize,
    pub items_fetched: usize,
}

#[derive(Debug, Error)]
pub enum DanbooruSyncError {
    #[error("无法连接 Danbooru: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Danbooru {resource} 分页游标没有前进，已停止同步")]
    StalledCursor { resource: &'static str },
    #[error("Danbooru {resource} 数据超过安全分页上限")]
    PageLimit { resource: &'static str },
}

pub fn fetch_artist_dictionary(
    mut progress: impl FnMut(ArtistDictionarySyncProgress),
) -> Result<ArtistDictionaryInput, DanbooruSyncError> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .build()?;

    let tags = fetch_pages::<DanbooruArtistTag>(
        &client,
        "画师 Tag",
        TAGS_QUERY,
        ArtistDictionarySyncStage::Tags,
        &mut progress,
    )?;
    let artists = fetch_pages::<DanbooruArtistRecord>(
        &client,
        "画师其它名称",
        ARTISTS_QUERY,
        ArtistDictionarySyncStage::Artists,
        &mut progress,
    )?;
    let aliases = fetch_pages::<DanbooruTagAlias>(
        &client,
        "画师别名",
        ALIASES_QUERY,
        ArtistDictionarySyncStage::Aliases,
        &mut progress,
    )?;

    Ok(ArtistDictionaryInput {
        tags,
        artists,
        aliases,
    })
}

fn fetch_pages<T>(
    client: &Client,
    resource: &'static str,
    query: &str,
    stage: ArtistDictionarySyncStage,
    progress: &mut impl FnMut(ArtistDictionarySyncProgress),
) -> Result<Vec<T>, DanbooruSyncError>
where
    T: DeserializeOwned + PageItem,
{
    let mut items = Vec::new();
    let mut cursor = None;
    for page_index in 0..MAX_PAGES_PER_RESOURCE {
        let url = page_url(query, cursor);
        let page = send_with_retry(client, &url)?.json::<Vec<T>>()?;
        if page.is_empty() {
            break;
        }
        let next_cursor = page.last().map(PageItem::id);
        if cursor.is_some() && next_cursor == cursor {
            return Err(DanbooruSyncError::StalledCursor { resource });
        }
        let page_len = page.len();
        items.extend(page);
        cursor = next_cursor;
        progress(ArtistDictionarySyncProgress {
            stage,
            pages_fetched: page_index + 1,
            items_fetched: items.len(),
        });
        if page_len < PAGE_SIZE {
            return Ok(items);
        }
        thread::sleep(Duration::from_millis(120));
    }

    Err(DanbooruSyncError::PageLimit { resource })
}

fn send_with_retry(client: &Client, url: &str) -> Result<Response, reqwest::Error> {
    let mut retry_delay = Duration::from_secs(1);
    for attempt in 0..4 {
        let response = client.get(url).send()?;
        if !matches!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS
                | StatusCode::BAD_GATEWAY
                | StatusCode::SERVICE_UNAVAILABLE
        ) || attempt == 3
        {
            return response.error_for_status();
        }
        thread::sleep(retry_delay);
        retry_delay *= 2;
    }
    unreachable!("retry loop always returns on its final iteration")
}

fn page_url(query: &str, cursor: Option<i64>) -> String {
    match cursor {
        Some(cursor) => format!("{API_ROOT}/{query}&page=b{cursor}"),
        None => format!("{API_ROOT}/{query}"),
    }
}

trait PageItem {
    fn id(&self) -> i64;
}

impl PageItem for DanbooruArtistTag {
    fn id(&self) -> i64 {
        self.id
    }
}

impl PageItem for DanbooruArtistRecord {
    fn id(&self) -> i64 {
        self.id
    }
}

impl PageItem for DanbooruTagAlias {
    fn id(&self) -> i64 {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_page_uses_danbooru_before_id_syntax() {
        assert_eq!(
            page_url("tags.json?limit=1000", Some(123)),
            "https://danbooru.donmai.us/tags.json?limit=1000&page=b123"
        );
    }

    #[test]
    fn initial_page_has_no_cursor_parameter() {
        assert_eq!(
            page_url("tags.json?limit=1000", None),
            "https://danbooru.donmai.us/tags.json?limit=1000"
        );
    }
}
