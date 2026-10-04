//! Local SQLite store for clipboard history and snippet usage statistics.
//! Usage stats are kept here rather than in the snippet files so that using
//! a snippet never touches (and re-syncs) the snippets directory.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

pub struct Db {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct ClipRow {
    pub id: i64,
    pub content: String,
    pub source: Option<String>,
    pub last_used_at: i64,
    pub use_count: u32,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Usage {
    pub use_count: u32,
    pub last_used_at: i64,
}

/// FNV-1a; only used to narrow duplicate lookups, content is compared too.
fn hash(text: &str) -> i64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h as i64
}

const CLIP_COLUMNS: &str = "id, content, source, last_used_at, use_count, pinned";

fn clip_from_row(row: &rusqlite::Row) -> rusqlite::Result<ClipRow> {
    Ok(ClipRow {
        id: row.get(0)?,
        content: row.get(1)?,
        source: row.get(2)?,
        last_used_at: row.get(3)?,
        use_count: row.get(4)?,
        pinned: row.get::<_, i64>(5)? != 0,
    })
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS clips (
                 id INTEGER PRIMARY KEY,
                 content TEXT NOT NULL,
                 hash INTEGER NOT NULL,
                 source TEXT,
                 created_at INTEGER NOT NULL,
                 last_used_at INTEGER NOT NULL,
                 use_count INTEGER NOT NULL DEFAULT 0,
                 pinned INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS clips_hash ON clips(hash);
             CREATE TABLE IF NOT EXISTS snippet_usage (
                 path TEXT PRIMARY KEY,
                 use_count INTEGER NOT NULL,
                 last_used_at INTEGER NOT NULL
             );",
        )?;
        Ok(Db { conn })
    }

    pub fn all_clips(&self) -> rusqlite::Result<Vec<ClipRow>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {CLIP_COLUMNS} FROM clips ORDER BY last_used_at DESC, id DESC"))?;
        let rows = stmt.query_map([], clip_from_row)?;
        rows.collect()
    }

    fn clip(&self, id: i64) -> rusqlite::Result<ClipRow> {
        self.conn.query_row(
            &format!("SELECT {CLIP_COLUMNS} FROM clips WHERE id = ?1"),
            [id],
            clip_from_row,
        )
    }

    /// Inserts a copied text, or moves an identical existing one to the top.
    pub fn record_clip(&self, content: &str, source: Option<&str>, now: i64) -> rusqlite::Result<ClipRow> {
        let h = hash(content);
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM clips WHERE hash = ?1 AND content = ?2",
                params![h, content],
                |r| r.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => {
                self.conn.execute(
                    "UPDATE clips SET last_used_at = ?2, source = COALESCE(?3, source) WHERE id = ?1",
                    params![id, now, source],
                )?;
                id
            }
            None => {
                self.conn.execute(
                    "INSERT INTO clips (content, hash, source, created_at, last_used_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                    params![content, h, source, now],
                )?;
                self.conn.last_insert_rowid()
            }
        };
        self.clip(id)
    }

    /// Deletes the oldest unpinned clips beyond `keep`; returns removed ids.
    pub fn prune(&self, keep: usize) -> rusqlite::Result<Vec<i64>> {
        let ids: Vec<i64> = {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM clips WHERE pinned = 0 ORDER BY last_used_at DESC, id DESC LIMIT -1 OFFSET ?1",
            )?;
            let rows = stmt.query_map([keep as i64], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for id in &ids {
            self.conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
        }
        Ok(ids)
    }

    pub fn touch_clip(&self, id: i64, now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE clips SET use_count = use_count + 1, last_used_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) -> rusqlite::Result<()> {
        self.conn
            .execute("UPDATE clips SET pinned = ?2 WHERE id = ?1", params![id, pinned as i64])?;
        Ok(())
    }

    pub fn delete_clip(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn snippet_usage(&self) -> rusqlite::Result<HashMap<String, Usage>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, use_count, last_used_at FROM snippet_usage")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Usage {
                    use_count: r.get(1)?,
                    last_used_at: r.get(2)?,
                },
            ))
        })?;
        rows.collect()
    }

    pub fn touch_snippet(&self, path: &str, now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO snippet_usage (path, use_count, last_used_at) VALUES (?1, 1, ?2)
             ON CONFLICT(path) DO UPDATE SET use_count = use_count + 1, last_used_at = ?2",
            params![path, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedups_and_prunes() {
        let db = Db::open(Path::new(":memory:")).unwrap();
        let a = db.record_clip("a", Some("x.exe"), 1).unwrap();
        db.record_clip("b", None, 2).unwrap();
        let a2 = db.record_clip("a", None, 3).unwrap();
        assert_eq!(a.id, a2.id);
        assert_eq!(a2.last_used_at, 3);
        assert_eq!(a2.source.as_deref(), Some("x.exe"));
        db.record_clip("c", None, 4).unwrap();
        db.set_pinned(a.id, true).unwrap();
        let removed = db.prune(1).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(db.all_clips().unwrap().len(), 2);
    }
}
