//! Local SQLite storage for settings, download history and the persistent queue.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::settings::Settings;

const SCHEMA_VERSION: i32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub thumbnail_local: Option<String>,
    pub platform: String,
    pub media_type: String,
    /// Output container or audio format, for example `mp4` or `mp3`.
    pub format: Option<String>,
    /// Resolution or audio quality label.
    pub quality: Option<String>,
    pub file_size: Option<u64>,
    pub file_path: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub file_exists: bool,
    #[serde(skip)]
    pub request_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistorySort {
    Newest,
    Oldest,
    Title,
    Size,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryQuery {
    pub search: Option<String>,
    pub platform: Option<String>,
    pub media_type: Option<String>,
    pub status: Option<String>,
    pub sort: HistorySort,
    pub limit: u32,
    pub offset: u32,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            search: None,
            platform: None,
            media_type: None,
            status: None,
            sort: HistorySort::Newest,
            limit: 100,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub items: Vec<HistoryEntry>,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCount {
    pub platform: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStats {
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub total_bytes: u64,
    pub by_platform: Vec<PlatformCount>,
    pub recent: Vec<HistoryEntry>,
}

pub struct Storage {
    conn: Mutex<Connection>,
}

fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for c in value.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

const HISTORY_COLUMNS: &str = "id, url, title, uploader, duration, thumbnail, thumbnail_local, platform, media_type, format, quality, \
     file_size, file_path, status, error, created_at, completed_at, request_json";

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    let file_path: Option<String> = row.get(12)?;
    let file_exists = file_path.as_deref().is_some_and(|p| Path::new(p).is_file());
    Ok(HistoryEntry {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        uploader: row.get(3)?,
        duration: row.get(4)?,
        thumbnail: row.get(5)?,
        thumbnail_local: row.get(6)?,
        platform: row.get(7)?,
        media_type: row.get(8)?,
        format: row.get(9)?,
        quality: row.get(10)?,
        file_size: row.get::<_, Option<i64>>(11)?.map(|s| s.max(0) as u64),
        file_path,
        status: row.get(13)?,
        error: row.get(14)?,
        created_at: row.get(15)?,
        completed_at: row.get(16)?,
        file_exists,
        request_json: row.get(17)?,
    })
}

impl Storage {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS history (
                    id TEXT PRIMARY KEY,
                    url TEXT NOT NULL,
                    title TEXT NOT NULL,
                    uploader TEXT,
                    duration REAL,
                    thumbnail TEXT,
                    thumbnail_local TEXT,
                    platform TEXT NOT NULL,
                    media_type TEXT NOT NULL,
                    format TEXT,
                    quality TEXT,
                    file_size INTEGER,
                    file_path TEXT,
                    status TEXT NOT NULL,
                    error TEXT,
                    created_at INTEGER NOT NULL,
                    completed_at INTEGER,
                    request_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_history_platform ON history(platform);
                 CREATE INDEX IF NOT EXISTS idx_history_status ON history(status);
                 CREATE INDEX IF NOT EXISTS idx_history_url ON history(url);
                 CREATE TABLE IF NOT EXISTS queue (id TEXT PRIMARY KEY, position INTEGER NOT NULL, json TEXT NOT NULL);
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        if version > SCHEMA_VERSION {
            return Err(AppError::internal(
                "The local database was created by a newer version of DarkLoad.",
            ));
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned lock only means another thread panicked mid-query; the connection is still usable.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ----- Settings -------------------------------------------------------------------------

    pub fn load_settings(&self) -> AppResult<Option<Settings>> {
        let json: Option<String> = self
            .conn()
            .query_row("SELECT json FROM settings WHERE id = 1", [], |r| r.get(0))
            .optional()?;
        Ok(json.and_then(|j| match parse_settings(&j) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("ignoring unreadable settings: {e}");
                None
            }
        }))
    }

    pub fn save_settings(&self, settings: &Settings) -> AppResult<()> {
        let json = serde_json::to_string(settings)?;
        self.conn().execute(
            "INSERT INTO settings (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![json],
        )?;
        Ok(())
    }

    // ----- History --------------------------------------------------------------------------

    pub fn upsert_history(&self, e: &HistoryEntry) -> AppResult<()> {
        self.conn().execute(
            &format!(
                "INSERT INTO history ({HISTORY_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title, uploader = excluded.uploader, duration = excluded.duration,
                   thumbnail = excluded.thumbnail, thumbnail_local = excluded.thumbnail_local, format = excluded.format,
                   quality = excluded.quality, file_size = excluded.file_size, file_path = excluded.file_path,
                   status = excluded.status, error = excluded.error, completed_at = excluded.completed_at,
                   request_json = excluded.request_json"
            ),
            params![
                e.id,
                e.url,
                e.title,
                e.uploader,
                e.duration,
                e.thumbnail,
                e.thumbnail_local,
                e.platform,
                e.media_type,
                e.format,
                e.quality,
                e.file_size.map(|s| s as i64),
                e.file_path,
                e.status,
                e.error,
                e.created_at,
                e.completed_at,
                e.request_json
            ],
        )?;
        Ok(())
    }

    pub fn get_history(&self, id: &str) -> AppResult<Option<HistoryEntry>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {HISTORY_COLUMNS} FROM history WHERE id = ?1"),
                params![id],
                row_to_entry,
            )
            .optional()?)
    }

    pub fn query_history(&self, q: &HistoryQuery) -> AppResult<HistoryPage> {
        let mut clauses: Vec<&str> = Vec::new();
        let mut values: Vec<String> = Vec::new();
        if let Some(search) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let pattern = format!(
                "%{}%",
                escape_like(&search.chars().take(200).collect::<String>())
            );
            clauses.push("(title LIKE ? ESCAPE '\\' OR uploader LIKE ? ESCAPE '\\' OR url LIKE ? ESCAPE '\\')");
            values.extend([pattern.clone(), pattern.clone(), pattern]);
        }
        if let Some(platform) = q.platform.as_deref().filter(|s| !s.is_empty()) {
            clauses.push("platform = ?");
            values.push(platform.to_string());
        }
        if let Some(media_type) = q.media_type.as_deref().filter(|s| !s.is_empty()) {
            clauses.push("media_type = ?");
            values.push(media_type.to_string());
        }
        if let Some(status) = q.status.as_deref().filter(|s| !s.is_empty()) {
            clauses.push("status = ?");
            values.push(status.to_string());
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        let order = match q.sort {
            HistorySort::Newest => "created_at DESC",
            HistorySort::Oldest => "created_at ASC",
            HistorySort::Title => "title COLLATE NOCASE ASC, created_at DESC",
            HistorySort::Size => "COALESCE(file_size, 0) DESC, created_at DESC",
        };
        let limit = q.limit.clamp(1, 500);
        let conn = self.conn();
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM history {where_sql}"),
            params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(&format!("SELECT {HISTORY_COLUMNS} FROM history {where_sql} ORDER BY {order} LIMIT {limit} OFFSET {}", q.offset))?;
        let items = stmt
            .query_map(params_from_iter(values.iter()), row_to_entry)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HistoryPage {
            items,
            total: total.max(0) as u64,
        })
    }

    pub fn delete_history(&self, ids: &[String]) -> AppResult<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut removed = 0;
        {
            let mut stmt = tx.prepare("DELETE FROM history WHERE id = ?1")?;
            for id in ids {
                removed += stmt.execute(params![id])?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    pub fn clear_history(&self) -> AppResult<usize> {
        Ok(self.conn().execute("DELETE FROM history", [])?)
    }

    /// The most recent completed download of `url` whose file still exists.
    pub fn find_completed(&self, url: &str) -> AppResult<Option<HistoryEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {HISTORY_COLUMNS} FROM history WHERE url = ?1 AND status = 'completed' ORDER BY created_at DESC LIMIT 5"
        ))?;
        let rows = stmt.query_map(params![url], row_to_entry)?;
        for row in rows {
            let entry = row?;
            if entry.file_exists {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    pub fn stats(&self) -> AppResult<HistoryStats> {
        let conn = self.conn();
        let count = |status: &str| -> AppResult<u64> {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM history WHERE status = ?1",
                params![status],
                |r| r.get(0),
            )?;
            Ok(n.max(0) as u64)
        };
        let completed = count("completed")?;
        let failed = count("failed")?;
        let cancelled = count("cancelled")?;
        let total_bytes: i64 = conn.query_row(
            "SELECT COALESCE(SUM(file_size), 0) FROM history WHERE status = 'completed'",
            [],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare("SELECT platform, COUNT(*) FROM history WHERE status = 'completed' GROUP BY platform ORDER BY 2 DESC")?;
        let by_platform = stmt
            .query_map([], |r| {
                Ok(PlatformCount {
                    platform: r.get(0)?,
                    count: r.get::<_, i64>(1)?.max(0) as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {HISTORY_COLUMNS} FROM history ORDER BY created_at DESC LIMIT 8"
        ))?;
        let recent = stmt
            .query_map([], row_to_entry)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HistoryStats {
            completed,
            failed,
            cancelled,
            total_bytes: total_bytes.max(0) as u64,
            by_platform,
            recent,
        })
    }

    // ----- Queue ----------------------------------------------------------------------------

    pub fn save_queue(&self, items: &[(String, String)]) -> AppResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM queue", [])?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO queue (id, position, json) VALUES (?1, ?2, ?3)")?;
            for (position, (id, json)) in items.iter().enumerate() {
                stmt.execute(params![id, position as i64, json])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_queue(&self) -> AppResult<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT json FROM queue ORDER BY position ASC")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Reads stored settings, upgrading fields from older versions.
fn parse_settings(json: &str) -> serde_json::Result<Settings> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    if let Some(obj) = value.as_object_mut() {
        // 1.0 stored a single "check for updates" switch; off becomes the "off" update mode.
        if let Some(auto) = obj.remove("autoCheckUpdates") {
            if !obj.contains_key("updateMode") && auto == serde_json::Value::Bool(false) {
                obj.insert("updateMode".into(), "off".into());
            }
        }
    }
    serde_json::from_value(value)
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrates_the_update_switch() {
        let off = parse_settings(r#"{"autoCheckUpdates":false}"#).unwrap();
        assert_eq!(off.update_mode, crate::updater::UpdateMode::Off);
        let on = parse_settings(r#"{"autoCheckUpdates":true}"#).unwrap();
        assert_eq!(on.update_mode, crate::updater::UpdateMode::Auto);
        let kept = parse_settings(r#"{"autoCheckUpdates":false,"updateMode":"notify"}"#).unwrap();
        assert_eq!(kept.update_mode, crate::updater::UpdateMode::Notify);
    }

    use super::*;

    fn entry(
        id: &str,
        title: &str,
        platform: &str,
        status: &str,
        created: i64,
        size: Option<u64>,
    ) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            url: format!("https://www.youtube.com/watch?v={id}"),
            title: title.into(),
            uploader: Some("Uploader".into()),
            duration: Some(10.0),
            thumbnail: None,
            thumbnail_local: None,
            platform: platform.into(),
            media_type: if id.starts_with('a') {
                "audio".into()
            } else {
                "video".into()
            },
            format: Some("mp4".into()),
            quality: Some("1080p".into()),
            file_size: size,
            file_path: None,
            status: status.into(),
            error: None,
            created_at: created,
            completed_at: Some(created + 5),
            file_exists: false,
            request_json: "{}".into(),
        }
    }

    fn seeded() -> Storage {
        let s = Storage::open_in_memory().unwrap();
        s.upsert_history(&entry(
            "v1",
            "Alpha 100% test",
            "youtube",
            "completed",
            1,
            Some(100),
        ))
        .unwrap();
        s.upsert_history(&entry("v2", "Beta_clip", "tiktok", "failed", 2, None))
            .unwrap();
        s.upsert_history(&entry(
            "a3",
            "Gamma song",
            "youtube",
            "completed",
            3,
            Some(50),
        ))
        .unwrap();
        s.upsert_history(&entry(
            "v4",
            "Ünïcödé 日本",
            "instagram",
            "cancelled",
            4,
            None,
        ))
        .unwrap();
        s
    }

    #[test]
    fn settings_round_trip() {
        let s = Storage::open_in_memory().unwrap();
        assert!(s.load_settings().unwrap().is_none());
        let mut settings = Settings {
            max_concurrent_downloads: 6,
            download_presets: vec![crate::settings::DownloadPreset {
                name: "Italian archive".into(),
                options: crate::formats::DownloadOptions {
                    audio_tracks: crate::formats::AudioTrackSelection {
                        original: true,
                        languages: vec!["it".into()],
                    },
                    download_subtitles: true,
                    ..Default::default()
                },
                output_dir: Some("downloads/archive".into()),
            }],
            ..Settings::default()
        };
        s.save_settings(&settings).unwrap();
        settings.theme = crate::settings::Theme::Dark;
        s.save_settings(&settings).unwrap();
        assert_eq!(s.load_settings().unwrap(), Some(settings));
    }

    #[test]
    fn settings_persist_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("darkload.db");
        {
            let s = Storage::open(&path).unwrap();
            let settings = Settings {
                download_dir: "/data/Видео".into(),
                ..Settings::default()
            };
            s.save_settings(&settings).unwrap();
            s.upsert_history(&entry(
                "v1",
                "Persisted",
                "youtube",
                "completed",
                1,
                Some(1),
            ))
            .unwrap();
        }
        let s = Storage::open(&path).unwrap();
        assert_eq!(
            s.load_settings().unwrap().unwrap().download_dir,
            "/data/Видео"
        );
        assert_eq!(s.query_history(&HistoryQuery::default()).unwrap().total, 1);
    }

    #[test]
    fn corrupt_settings_fall_back() {
        let s = Storage::open_in_memory().unwrap();
        s.conn()
            .execute("INSERT INTO settings (id, json) VALUES (1, 'not json')", [])
            .unwrap();
        assert!(s.load_settings().unwrap().is_none());
    }

    #[test]
    fn queries_filter_and_sort() {
        let s = seeded();
        let all = s.query_history(&HistoryQuery::default()).unwrap();
        assert_eq!(all.total, 4);
        assert_eq!(all.items[0].id, "v4", "newest first");
        let oldest = s
            .query_history(&HistoryQuery {
                sort: HistorySort::Oldest,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(oldest.items[0].id, "v1");
        let by_size = s
            .query_history(&HistoryQuery {
                sort: HistorySort::Size,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_size.items[0].id, "v1");
        let yt = s
            .query_history(&HistoryQuery {
                platform: Some("youtube".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(yt.total, 2);
        let audio = s
            .query_history(&HistoryQuery {
                media_type: Some("audio".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(audio.items[0].id, "a3");
        let failed = s
            .query_history(&HistoryQuery {
                status: Some("failed".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(failed.items[0].id, "v2");
        let paged = s
            .query_history(&HistoryQuery {
                limit: 1,
                offset: 1,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(paged.items.len(), 1);
        assert_eq!(paged.total, 4);
    }

    #[test]
    fn search_escapes_wildcards_and_supports_unicode() {
        let s = seeded();
        let r = s
            .query_history(&HistoryQuery {
                search: Some("100%".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(r.total, 1);
        let r = s
            .query_history(&HistoryQuery {
                search: Some("_".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            r.items.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["v2"]
        );
        let r = s
            .query_history(&HistoryQuery {
                search: Some("日本".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(r.total, 1);
        let r = s
            .query_history(&HistoryQuery {
                search: Some("' OR 1=1 --".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(r.total, 0);
    }

    #[test]
    fn delete_and_clear_do_not_touch_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("kept.mp4");
        std::fs::write(&file, b"data").unwrap();
        let s = seeded();
        let mut e = entry("v9", "With file", "youtube", "completed", 9, Some(4));
        e.file_path = Some(file.to_string_lossy().into_owned());
        s.upsert_history(&e).unwrap();
        assert!(s.get_history("v9").unwrap().unwrap().file_exists);
        assert_eq!(
            s.delete_history(&["v9".into(), "missing".into()]).unwrap(),
            1
        );
        assert!(file.exists(), "removing history must not delete media");
        assert_eq!(s.clear_history().unwrap(), 4);
        assert_eq!(s.query_history(&HistoryQuery::default()).unwrap().total, 0);
    }

    #[test]
    fn stats_use_real_data() {
        let s = seeded();
        let stats = s.stats().unwrap();
        assert_eq!(stats.completed, 2);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.cancelled, 1);
        assert_eq!(stats.total_bytes, 150);
        assert_eq!(
            stats.by_platform,
            vec![PlatformCount {
                platform: "youtube".into(),
                count: 2
            }]
        );
        assert_eq!(stats.recent.len(), 4);
    }

    #[test]
    fn duplicate_detection_requires_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = seeded();
        assert!(s
            .find_completed("https://www.youtube.com/watch?v=v1")
            .unwrap()
            .is_none());
        let file = dir.path().join("v1.mp4");
        std::fs::write(&file, b"x").unwrap();
        let mut e = entry("v1", "Alpha", "youtube", "completed", 10, Some(1));
        e.file_path = Some(file.to_string_lossy().into_owned());
        s.upsert_history(&e).unwrap();
        assert_eq!(
            s.find_completed("https://www.youtube.com/watch?v=v1")
                .unwrap()
                .unwrap()
                .id,
            "v1"
        );
    }

    #[test]
    fn queue_round_trip_preserves_order() {
        let s = Storage::open_in_memory().unwrap();
        s.save_queue(&[
            ("b".into(), "{\"n\":2}".into()),
            ("a".into(), "{\"n\":1}".into()),
        ])
        .unwrap();
        assert_eq!(
            s.load_queue().unwrap(),
            vec!["{\"n\":2}".to_string(), "{\"n\":1}".to_string()]
        );
        s.save_queue(&[]).unwrap();
        assert!(s.load_queue().unwrap().is_empty());
    }

    #[test]
    fn handles_large_histories() {
        let s = Storage::open_in_memory().unwrap();
        for i in 0..5000 {
            s.upsert_history(&entry(
                &format!("v{i}"),
                &format!("Item {i}"),
                "youtube",
                "completed",
                i,
                Some(1),
            ))
            .unwrap();
        }
        let started = std::time::Instant::now();
        let page = s
            .query_history(&HistoryQuery {
                search: Some("Item 49".into()),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 111);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
