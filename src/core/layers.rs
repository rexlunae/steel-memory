use std::path::PathBuf;
use crate::types::Drawer;
use crate::core::dialect::compress_to_aaak;
use crate::storage::vector::VectorStorage;

pub struct MemoryStack {
    pub palace_path: PathBuf,
    pub db_path: PathBuf,
}

impl MemoryStack {
    pub fn new(palace_path: PathBuf, db_path: PathBuf) -> Self {
        Self { palace_path, db_path }
    }

    pub async fn layer0(&self) -> String {
        let identity_path = self.palace_path.join("identity.txt");
        tokio::fs::read_to_string(&identity_path)
            .await
            .unwrap_or_else(|_| "No identity file found.".to_string())
    }

    pub async fn layer1(&self, wing: Option<&str>, limit: usize) -> anyhow::Result<String> {
        let db_path = self.db_path.clone();
        let wing_owned = wing.map(|s| s.to_string());
        let drawers: Vec<Drawer> = tokio::task::spawn_blocking(move || {
            let storage = VectorStorage::new(&db_path)?;
            storage.get_all(wing_owned.as_deref(), None, limit)
        })
        .await??;

        if drawers.is_empty() {
            return Ok("(no memories yet)".to_string());
        }

        let aaak_lines: Vec<String> = drawers
            .iter()
            .map(|d| format!("[{}] {}", d.id, compress_to_aaak(d)))
            .collect();
        Ok(aaak_lines.join("\n---\n"))
    }

    pub async fn wake_up(&self, wing: Option<&str>) -> anyhow::Result<String> {
        let l0 = self.layer0().await;
        let l1 = self.layer1(wing, 20).await?;
        Ok(format!(
            "=== LAYER 0: IDENTITY ===\n{}\n\n=== LAYER 1: AAAK MEMORIES ===\n{}",
            l0, l1
        ))
    }

    pub async fn recall(
        &self,
        wing: Option<&str>,
        room: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<Drawer>> {
        let db_path = self.db_path.clone();
        let wing_owned = wing.map(|s| s.to_string());
        let room_owned = room.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            let storage = VectorStorage::new(&db_path)?;
            storage.get_all(wing_owned.as_deref(), room_owned.as_deref(), limit)
        })
        .await?
    }
}
