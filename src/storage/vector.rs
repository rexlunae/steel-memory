use std::path::{Path, PathBuf};
use rusqlite::{Connection, params};
use crate::types::{Drawer, SearchResult};

pub struct VectorStorage {
    db_path: PathBuf,
}

impl VectorStorage {
    pub fn new(db_path: &Path) -> anyhow::Result<Self> {
        let conn = open_conn(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS drawers (
                 id TEXT PRIMARY KEY,
                 content TEXT NOT NULL,
                 wing TEXT NOT NULL,
                 room TEXT NOT NULL,
                 source_file TEXT NOT NULL DEFAULT 'mcp',
                 source_mtime INTEGER NOT NULL DEFAULT 0,
                 chunk_index INTEGER NOT NULL DEFAULT 0,
                 added_by TEXT NOT NULL DEFAULT 'mcp',
                 filed_at TEXT NOT NULL,
                 hall TEXT DEFAULT '',
                 topic TEXT DEFAULT '',
                 drawer_type TEXT DEFAULT '',
                 agent TEXT DEFAULT '',
                 date TEXT DEFAULT '',
                 importance REAL DEFAULT 3.0,
                 vector TEXT NOT NULL
             );",
        )?;
        Ok(Self { db_path: db_path.to_path_buf() })
    }

    pub fn add_drawer(&self, drawer: &Drawer, vector: &[f32]) -> anyhow::Result<()> {
        let conn = open_conn(&self.db_path)?;
        let vec_json = serde_json::to_string(vector)?;
        conn.execute(
            "INSERT OR REPLACE INTO drawers
             (id, content, wing, room, source_file, source_mtime, chunk_index, added_by,
              filed_at, hall, topic, drawer_type, agent, date, importance, vector)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                drawer.id, drawer.content, drawer.wing, drawer.room,
                drawer.source_file, drawer.source_mtime, drawer.chunk_index, drawer.added_by,
                drawer.filed_at, drawer.hall, drawer.topic, drawer.drawer_type,
                drawer.agent, drawer.date, drawer.importance, vec_json
            ],
        )?;
        Ok(())
    }

    pub fn delete_drawer(&self, id: &str) -> anyhow::Result<usize> {
        let conn = open_conn(&self.db_path)?;
        let n = conn.execute("DELETE FROM drawers WHERE id = ?1", params![id])?;
        Ok(n)
    }

    pub fn search(
        &self,
        query_vec: &[f32],
        limit: usize,
        wing: Option<&str>,
        room: Option<&str>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, content, wing, room, source_file, source_mtime, chunk_index,
                    added_by, filed_at, hall, topic, drawer_type, agent, date, importance, vector
             FROM drawers",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row_to_drawer(row)?,
                row.get::<_, String>(15)?,
            ))
        })?;

        let mut results: Vec<SearchResult> = Vec::new();
        for row in rows {
            let (drawer, vec_json) = row?;
            if let Some(w) = wing {
                if drawer.wing != w { continue; }
            }
            if let Some(r) = room {
                if drawer.room != r { continue; }
            }
            let stored_vec: Vec<f32> = serde_json::from_str(&vec_json).unwrap_or_default();
            let sim = cosine_similarity(query_vec, &stored_vec);
            results.push(SearchResult { drawer, similarity: sim });
        }
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
        results.truncate(limit);
        Ok(results)
    }

    pub fn get_all(
        &self,
        wing: Option<&str>,
        room: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<Drawer>> {
        let conn = open_conn(&self.db_path)?;
        let (sql, param_wing, param_room) = match (wing, room) {
            (Some(w), Some(r)) => (
                "SELECT id,content,wing,room,source_file,source_mtime,chunk_index,added_by,filed_at,hall,topic,drawer_type,agent,date,importance,vector FROM drawers WHERE wing=?1 AND room=?2 ORDER BY importance DESC LIMIT ?3",
                Some(w.to_string()), Some(r.to_string())
            ),
            (Some(w), None) => (
                "SELECT id,content,wing,room,source_file,source_mtime,chunk_index,added_by,filed_at,hall,topic,drawer_type,agent,date,importance,vector FROM drawers WHERE wing=?1 ORDER BY importance DESC LIMIT ?3",
                Some(w.to_string()), None
            ),
            (None, Some(r)) => (
                "SELECT id,content,wing,room,source_file,source_mtime,chunk_index,added_by,filed_at,hall,topic,drawer_type,agent,date,importance,vector FROM drawers WHERE room=?2 ORDER BY importance DESC LIMIT ?3",
                None, Some(r.to_string())
            ),
            (None, None) => (
                "SELECT id,content,wing,room,source_file,source_mtime,chunk_index,added_by,filed_at,hall,topic,drawer_type,agent,date,importance,vector FROM drawers ORDER BY importance DESC LIMIT ?3",
                None, None
            ),
        };
        let mut stmt = conn.prepare(sql)?;
        let drawers = stmt.query_map(
            rusqlite::params_from_iter([
                param_wing.as_deref().unwrap_or(""),
                param_room.as_deref().unwrap_or(""),
                &limit.to_string(),
            ]),
            |row| row_to_drawer(row),
        )?.filter_map(|r| r.ok()).collect();
        Ok(drawers)
    }

    pub fn count(&self) -> anyhow::Result<usize> {
        let conn = open_conn(&self.db_path)?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM drawers", [], |row| row.get(0))?;
        Ok(n as usize)
    }

    pub fn list_wings(&self) -> anyhow::Result<Vec<(String, usize)>> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn.prepare("SELECT wing, COUNT(*) as cnt FROM drawers GROUP BY wing ORDER BY cnt DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }

    pub fn list_rooms(&self, wing: Option<&str>) -> anyhow::Result<Vec<(String, String, usize)>> {
        let conn = open_conn(&self.db_path)?;
        let rows = if let Some(w) = wing {
            let mut stmt = conn.prepare(
                "SELECT wing, room, COUNT(*) as cnt FROM drawers WHERE wing=?1 GROUP BY wing, room ORDER BY cnt DESC"
            )?;
            stmt.query_map(params![w], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)? as usize))
            })?.filter_map(|r| r.ok()).collect()
        } else {
            let mut stmt = conn.prepare(
                "SELECT wing, room, COUNT(*) as cnt FROM drawers GROUP BY wing, room ORDER BY cnt DESC"
            )?;
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)? as usize))
            })?.filter_map(|r| r.ok()).collect()
        };
        Ok(rows)
    }

    pub fn get_taxonomy(&self) -> anyhow::Result<serde_json::Value> {
        let rooms = self.list_rooms(None)?;
        let mut map = serde_json::Map::new();
        for (wing, room, count) in rooms {
            let wing_entry = map.entry(wing).or_insert_with(|| serde_json::json!({}));
            if let serde_json::Value::Object(wing_map) = wing_entry {
                wing_map.insert(room, serde_json::json!(count));
            }
        }
        Ok(serde_json::Value::Object(map))
    }
}

fn open_conn(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

fn row_to_drawer(row: &rusqlite::Row<'_>) -> rusqlite::Result<Drawer> {
    Ok(Drawer {
        id: row.get(0)?,
        content: row.get(1)?,
        wing: row.get(2)?,
        room: row.get(3)?,
        source_file: row.get(4)?,
        source_mtime: row.get(5)?,
        chunk_index: row.get(6)?,
        added_by: row.get(7)?,
        filed_at: row.get(8)?,
        hall: row.get(9)?,
        topic: row.get(10)?,
        drawer_type: row.get(11)?,
        agent: row.get(12)?,
        date: row.get(13)?,
        importance: row.get(14)?,
    })
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}
