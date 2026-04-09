use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drawer {
    pub id: String,
    pub content: String,
    pub wing: String,
    pub room: String,
    pub source_file: String,
    pub source_mtime: i64,
    pub chunk_index: i64,
    pub added_by: String,
    pub filed_at: String,
    pub hall: String,
    pub topic: String,
    pub drawer_type: String,
    pub agent: String,
    pub date: String,
    pub importance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub drawer: Drawer,
    pub similarity: f32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub properties: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Triple {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub source_closet: Option<String>,
    pub source_file: Option<String>,
    pub extracted_at: String,
}
