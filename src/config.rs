use std::path::PathBuf;

#[allow(dead_code)]
pub struct Config {
    pub palace_path: PathBuf,
    pub identity_file: PathBuf,
    pub vector_db: PathBuf,
    pub kg_db: PathBuf,
    pub collection: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let home = dirs_or_manual_home();
        let palace = home.join(".steel-memory");
        std::fs::create_dir_all(&palace)?;
        Ok(Self {
            identity_file: palace.join("identity.txt"),
            vector_db: palace.join("palace.sqlite3"),
            kg_db: palace.join("knowledge_graph.sqlite3"),
            collection: std::env::var("STEEL_MEMORY_COLLECTION")
                .unwrap_or_else(|_| "drawers".to_string()),
            palace_path: palace,
        })
    }
}

fn dirs_or_manual_home() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    #[cfg(target_os = "windows")]
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        return PathBuf::from(userprofile);
    }
    PathBuf::from(".")
}
