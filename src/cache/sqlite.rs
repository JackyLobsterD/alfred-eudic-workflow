use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rusqlite::{Connection, OpenFlags, params};

use super::{CACHE_TTL_SECS, Cache, CacheKind};

pub struct SqliteCache {
    conn: Mutex<Connection>,
}

impl SqliteCache {
    pub fn open<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_millis(500))?;
        Self::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn migrate(conn: &Connection) -> rusqlite::Result<()> {
        for kind in [CacheKind::Urban, CacheKind::Wordnik, CacheKind::Llm] {
            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {} (\
                    key TEXT PRIMARY KEY, \
                    value BLOB NOT NULL, \
                    fetched_at INTEGER NOT NULL\
                 )",
                kind.table()
            );
            conn.execute(&sql, [])?;
        }
        Ok(())
    }
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[async_trait]
impl Cache for SqliteCache {
    async fn get(&self, kind: CacheKind, key: &str) -> Option<Vec<u8>> {
        let conn = self.conn.lock().ok()?;
        let sql = format!("SELECT value, fetched_at FROM {} WHERE key = ?1", kind.table());
        let mut stmt = conn.prepare(&sql).ok()?;
        let mut rows = stmt.query(params![key]).ok()?;
        let row = rows.next().ok()??;
        let value: Vec<u8> = row.get(0).ok()?;
        let fetched_at: i64 = row.get(1).ok()?;
        if now_secs() - fetched_at > CACHE_TTL_SECS {
            return None;
        }
        Some(value)
    }

    async fn put(&self, kind: CacheKind, key: &str, value: &[u8]) {
        let Ok(conn) = self.conn.lock() else { return };
        let sql = format!(
            "INSERT INTO {} (key, value, fetched_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, fetched_at=excluded.fetched_at",
            kind.table()
        );
        let _ = conn.execute(&sql, params![key, value, now_secs()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_then_get_roundtrip() {
        let c = SqliteCache::in_memory().unwrap();
        c.put(CacheKind::Urban, "hello", b"world").await;
        let v = c.get(CacheKind::Urban, "hello").await;
        assert_eq!(v.as_deref(), Some(&b"world"[..]));
    }

    #[tokio::test]
    async fn kinds_are_isolated() {
        let c = SqliteCache::in_memory().unwrap();
        c.put(CacheKind::Urban, "k", b"u").await;
        c.put(CacheKind::Wordnik, "k", b"w").await;
        assert_eq!(c.get(CacheKind::Urban, "k").await.as_deref(), Some(&b"u"[..]));
        assert_eq!(c.get(CacheKind::Wordnik, "k").await.as_deref(), Some(&b"w"[..]));
        assert_eq!(c.get(CacheKind::Llm, "k").await, None);
    }

    #[tokio::test]
    async fn miss_returns_none() {
        let c = SqliteCache::in_memory().unwrap();
        assert_eq!(c.get(CacheKind::Urban, "missing").await, None);
    }

    #[tokio::test]
    async fn put_overwrites() {
        let c = SqliteCache::in_memory().unwrap();
        c.put(CacheKind::Urban, "k", b"v1").await;
        c.put(CacheKind::Urban, "k", b"v2").await;
        assert_eq!(c.get(CacheKind::Urban, "k").await.as_deref(), Some(&b"v2"[..]));
    }

    #[tokio::test]
    async fn expired_entry_returns_none() {
        let c = SqliteCache::in_memory().unwrap();
        // Insert directly with old timestamp.
        {
            let conn = c.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO cache_urban (key, value, fetched_at) VALUES (?1, ?2, ?3)",
                params!["old", &b"v"[..], now_secs() - CACHE_TTL_SECS - 1],
            ).unwrap();
        }
        assert_eq!(c.get(CacheKind::Urban, "old").await, None);
    }
}
