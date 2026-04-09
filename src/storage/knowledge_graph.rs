use std::path::{Path, PathBuf};
use rusqlite::{Connection, params};
use crate::types::{Entity, Triple};

pub struct KnowledgeGraph {
    db_path: PathBuf,
}

impl KnowledgeGraph {
    pub fn new(db_path: &Path) -> anyhow::Result<Self> {
        let conn = open_conn(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS entities (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 entity_type TEXT DEFAULT 'unknown',
                 properties TEXT DEFAULT '{}',
                 created_at TEXT DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS triples (
                 id TEXT PRIMARY KEY,
                 subject TEXT NOT NULL,
                 predicate TEXT NOT NULL,
                 object TEXT NOT NULL,
                 valid_from TEXT,
                 valid_to TEXT,
                 confidence REAL DEFAULT 1.0,
                 source_closet TEXT,
                 source_file TEXT,
                 extracted_at TEXT DEFAULT CURRENT_TIMESTAMP
             );
             CREATE INDEX IF NOT EXISTS idx_triples_subject ON triples(subject);
             CREATE INDEX IF NOT EXISTS idx_triples_object ON triples(object);",
        )?;
        Ok(Self { db_path: db_path.to_path_buf() })
    }

    pub fn add_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        confidence: f64,
        source_closet: Option<&str>,
        source_file: Option<&str>,
    ) -> anyhow::Result<String> {
        let conn = open_conn(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();

        // Ensure entities exist
        let subj_id = normalize_id(subject);
        let obj_id = normalize_id(object);

        upsert_entity(&conn, &subj_id, subject)?;
        upsert_entity(&conn, &obj_id, object)?;

        let pred_norm = normalize_id(predicate);
        let ts = chrono::Utc::now().timestamp_millis();
        let triple_id = format!("triple_{subj_id}_{pred_norm}_{obj_id}_{ts}");

        conn.execute(
            "INSERT OR REPLACE INTO triples
             (id, subject, predicate, object, valid_from, confidence, source_closet, source_file, extracted_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                triple_id, subj_id, predicate, obj_id,
                now, confidence, source_closet, source_file, now
            ],
        )?;
        Ok(triple_id)
    }

    pub fn invalidate_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
    ) -> anyhow::Result<usize> {
        let conn = open_conn(&self.db_path)?;
        let now = chrono::Utc::now().to_rfc3339();
        let subj_id = normalize_id(subject);
        let obj_id = normalize_id(object);
        let n = conn.execute(
            "UPDATE triples SET valid_to=?1 WHERE subject=?2 AND predicate=?3 AND object=?4 AND valid_to IS NULL",
            params![now, subj_id, predicate, obj_id],
        )?;
        Ok(n)
    }

    pub fn query_entity(
        &self,
        entity: &str,
        direction: &str,
    ) -> anyhow::Result<Vec<Triple>> {
        let conn = open_conn(&self.db_path)?;
        let entity_id = normalize_id(entity);
        let triples = match direction {
            "outgoing" => {
                let mut stmt = conn.prepare(
                    "SELECT id,subject,predicate,object,valid_from,valid_to,confidence,source_closet,source_file,extracted_at FROM triples WHERE subject=?1 AND valid_to IS NULL ORDER BY extracted_at DESC"
                )?;
                stmt.query_map(params![entity_id], row_to_triple)?
                    .filter_map(|r| r.ok())
                    .collect()
            }
            "incoming" => {
                let mut stmt = conn.prepare(
                    "SELECT id,subject,predicate,object,valid_from,valid_to,confidence,source_closet,source_file,extracted_at FROM triples WHERE object=?1 AND valid_to IS NULL ORDER BY extracted_at DESC"
                )?;
                stmt.query_map(params![entity_id], row_to_triple)?
                    .filter_map(|r| r.ok())
                    .collect()
            }
            _ => {
                let mut stmt = conn.prepare(
                    "SELECT id,subject,predicate,object,valid_from,valid_to,confidence,source_closet,source_file,extracted_at FROM triples WHERE (subject=?1 OR object=?1) AND valid_to IS NULL ORDER BY extracted_at DESC"
                )?;
                stmt.query_map(params![entity_id], row_to_triple)?
                    .filter_map(|r| r.ok())
                    .collect()
            }
        };
        Ok(triples)
    }

    pub fn timeline(&self, entity: &str, limit: usize) -> anyhow::Result<Vec<Triple>> {
        let conn = open_conn(&self.db_path)?;
        let entity_id = normalize_id(entity);
        let mut stmt = conn.prepare(
            "SELECT id,subject,predicate,object,valid_from,valid_to,confidence,source_closet,source_file,extracted_at FROM triples WHERE subject=?1 OR object=?1 ORDER BY extracted_at DESC LIMIT ?2"
        )?;
        let triples = stmt.query_map(params![entity_id, limit as i64], row_to_triple)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(triples)
    }

    pub fn all_triples(&self, limit: usize) -> anyhow::Result<Vec<Triple>> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id,subject,predicate,object,valid_from,valid_to,confidence,source_closet,source_file,extracted_at FROM triples ORDER BY extracted_at DESC LIMIT ?1"
        )?;
        let triples = stmt.query_map(params![limit as i64], row_to_triple)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(triples)
    }

    pub fn stats(&self) -> anyhow::Result<serde_json::Value> {
        let conn = open_conn(&self.db_path)?;
        let entity_count: i64 = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
        let triple_count: i64 = conn.query_row("SELECT COUNT(*) FROM triples", [], |r| r.get(0))?;
        let valid_count: i64 = conn.query_row("SELECT COUNT(*) FROM triples WHERE valid_to IS NULL", [], |r| r.get(0))?;
        Ok(serde_json::json!({
            "entities": entity_count,
            "triples": triple_count,
            "valid_triples": valid_count,
        }))
    }

    #[allow(dead_code)]
    pub fn list_entities(&self, limit: usize) -> anyhow::Result<Vec<Entity>> {
        let conn = open_conn(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id,name,entity_type,properties,created_at FROM entities ORDER BY created_at DESC LIMIT ?1"
        )?;
        let entities = stmt.query_map(params![limit as i64], |row| {
            Ok(Entity {
                id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                properties: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                created_at: row.get(4)?,
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok(entities)
    }
}

fn upsert_entity(conn: &Connection, id: &str, name: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO entities (id, name) VALUES (?1, ?2)",
        params![id, name],
    )?;
    Ok(())
}

fn open_conn(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

fn row_to_triple(row: &rusqlite::Row<'_>) -> rusqlite::Result<Triple> {
    Ok(Triple {
        id: row.get(0)?,
        subject: row.get(1)?,
        predicate: row.get(2)?,
        object: row.get(3)?,
        valid_from: row.get(4)?,
        valid_to: row.get(5)?,
        confidence: row.get(6)?,
        source_closet: row.get(7)?,
        source_file: row.get(8)?,
        extracted_at: row.get(9)?,
    })
}

pub fn normalize_id(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut result = String::new();
    let mut prev_was_underscore = false;
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
            prev_was_underscore = false;
        } else if !prev_was_underscore {
            result.push('_');
            prev_was_underscore = true;
        }
    }
    result.trim_matches('_').to_string()
}
