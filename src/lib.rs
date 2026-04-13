pub mod benchmark;
pub mod config;
pub mod core;
pub mod error;
pub mod storage;
pub mod types;

// Re-exports for library users
pub use core::dialect::compress_to_aaak;
pub use storage::knowledge_graph::normalize_id;
pub use storage::vector::{cosine_similarity, VectorStorage};
pub use types::{Drawer, SearchResult};

use std::path::PathBuf;

pub struct TestConfig {
    pub vector_db: PathBuf,
    pub kg_db: PathBuf,
    pub palace_path: PathBuf,
    pub identity_file: PathBuf,
}

pub fn config_default_with_base(base: PathBuf) -> TestConfig {
    TestConfig {
        vector_db: base.join("palace.sqlite3"),
        kg_db: base.join("knowledge_graph.sqlite3"),
        identity_file: base.join("identity.txt"),
        palace_path: base,
    }
}
