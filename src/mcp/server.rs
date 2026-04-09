use std::{path::PathBuf, sync::Arc};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    core::{aaak_spec::AAAK_SPEC, layers::MemoryStack},
    storage::{
        knowledge_graph::KnowledgeGraph,
        palace_graph::PalaceGraph,
        vector::VectorStorage,
    },
    types::Drawer,
};

// ── parameter structs ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
struct ListRoomsParams {
    wing: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchParams {
    query: String,
    limit: Option<u32>,
    wing: Option<String>,
    room: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CheckDuplicateParams {
    content: String,
    threshold: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WakeUpParams {
    wing: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RecallParams {
    wing: Option<String>,
    room: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TraverseGraphParams {
    start_room: String,
    max_hops: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FindTunnelsParams {
    wing_a: Option<String>,
    wing_b: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddDrawerParams {
    wing: String,
    room: String,
    content: String,
    topic: Option<String>,
    importance: Option<f64>,
    agent: Option<String>,
    hall: Option<String>,
    drawer_type: Option<String>,
    date: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DeleteDrawerParams {
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KgQueryParams {
    entity: String,
    direction: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KgAddParams {
    subject: String,
    predicate: String,
    object: String,
    confidence: Option<f64>,
    source_closet: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KgInvalidateParams {
    subject: String,
    predicate: String,
    object: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KgTimelineParams {
    entity: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DiaryWriteParams {
    agent_name: String,
    entry: String,
    topic: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DiaryReadParams {
    agent_name: String,
    limit: Option<u32>,
}

// ── main struct ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct MemPalace {
    tool_router: ToolRouter<MemPalace>,
    db_path: PathBuf,
    kg_path: PathBuf,
    palace_path: PathBuf,
    embedding: Arc<std::sync::Mutex<TextEmbedding>>,
}

#[tool_router]
impl MemPalace {
    pub async fn new() -> anyhow::Result<Self> {
        let config = crate::config::Config::default()?;

        VectorStorage::new(&config.vector_db)?;
        KnowledgeGraph::new(&config.kg_db)?;

        let model = tokio::task::spawn_blocking(|| {
            TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
                .map_err(|e| anyhow::anyhow!("{}", e))
        })
        .await??;

        Ok(Self {
            tool_router: Self::tool_router(),
            db_path: config.vector_db,
            kg_path: config.kg_db,
            palace_path: config.palace_path,
            embedding: Arc::new(std::sync::Mutex::new(model)),
        })
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let model = self.embedding.clone();
        let text_owned = text.to_string();
        tokio::task::spawn_blocking(move || {
            let mut model = model.lock().map_err(|e| anyhow::anyhow!("lock: {}", e))?;
            let mut embeddings = model
                .embed(vec![text_owned.as_str()], None)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            Ok::<Vec<f32>, anyhow::Error>(embeddings.remove(0))
        })
        .await?
    }

    // ── tool 1: status ───────────────────────────────────────────────────────

    #[tool(description = "Returns total_drawers count and palace_path")]
    async fn mempalace_status(&self) -> String {
        let db_path = self.db_path.clone();
        let palace_path = self.palace_path.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let storage = VectorStorage::new(&db_path)?;
            let count = storage.count()?;
            Ok((count, palace_path.to_string_lossy().to_string()))
        })
        .await;
        match result {
            Ok(Ok((count, path))) => serde_json::json!({
                "total_drawers": count,
                "palace_path": path,
            })
            .to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 2: list_wings ───────────────────────────────────────────────────

    #[tool(description = "List all wings (namespaces) with drawer counts")]
    async fn list_wings(&self) -> String {
        let db_path = self.db_path.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.list_wings()
        })
        .await;
        match result {
            Ok(Ok(wings)) => {
                let items: Vec<serde_json::Value> = wings
                    .iter()
                    .map(|(w, c)| serde_json::json!({"wing": w, "count": c}))
                    .collect();
                serde_json::json!({ "wings": items }).to_string()
            }
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 3: list_rooms ───────────────────────────────────────────────────

    #[tool(description = "List rooms, optionally filtered by wing")]
    async fn list_rooms(&self, Parameters(p): Parameters<ListRoomsParams>) -> String {
        let db_path = self.db_path.clone();
        let wing = p.wing.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.list_rooms(wing.as_deref())
        })
        .await;
        match result {
            Ok(Ok(rooms)) => {
                let items: Vec<serde_json::Value> = rooms
                    .iter()
                    .map(|(w, r, c)| serde_json::json!({"wing": w, "room": r, "count": c}))
                    .collect();
                serde_json::json!({ "rooms": items }).to_string()
            }
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 4: get_taxonomy ─────────────────────────────────────────────────

    #[tool(description = "Get full wing/room taxonomy with counts")]
    async fn get_taxonomy(&self) -> String {
        let db_path = self.db_path.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.get_taxonomy()
        })
        .await;
        match result {
            Ok(Ok(tax)) => serde_json::json!({ "taxonomy": tax }).to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 5: search ───────────────────────────────────────────────────────

    #[tool(description = "Semantic search over drawers using embedding similarity")]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> String {
        let vec = match self.embed(&p.query).await {
            Ok(v) => v,
            Err(e) => return serde_json::json!({ "error": e.to_string() }).to_string(),
        };
        let db_path = self.db_path.clone();
        let limit = p.limit.unwrap_or(10) as usize;
        let wing = p.wing.clone();
        let room = p.room.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.search(&vec, limit, wing.as_deref(), room.as_deref())
        })
        .await;
        match result {
            Ok(Ok(results)) => serde_json::to_string(&results)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 6: check_duplicate ──────────────────────────────────────────────

    #[tool(description = "Check if content is a near-duplicate of an existing drawer")]
    async fn check_duplicate(&self, Parameters(p): Parameters<CheckDuplicateParams>) -> String {
        let vec = match self.embed(&p.content).await {
            Ok(v) => v,
            Err(e) => return serde_json::json!({ "error": e.to_string() }).to_string(),
        };
        let threshold = p.threshold.unwrap_or(0.95) as f32;
        let db_path = self.db_path.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.search(&vec, 1, None, None)
        })
        .await;
        match result {
            Ok(Ok(results)) => {
                if let Some(top) = results.first() {
                    let is_dup = top.similarity >= threshold;
                    serde_json::json!({
                        "is_duplicate": is_dup,
                        "similarity": top.similarity,
                        "top_match_id": top.drawer.id,
                    })
                    .to_string()
                } else {
                    serde_json::json!({ "is_duplicate": false }).to_string()
                }
            }
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 7: wake_up ──────────────────────────────────────────────────────

    #[tool(description = "Return identity layer and AAAK-compressed top memories for context priming")]
    async fn wake_up(&self, Parameters(p): Parameters<WakeUpParams>) -> String {
        let stack = MemoryStack::new(self.palace_path.clone(), self.db_path.clone());
        match stack.wake_up(p.wing.as_deref()).await {
            Ok(s) => serde_json::json!({ "context": s }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 8: recall ───────────────────────────────────────────────────────

    #[tool(description = "List drawers by wing/room with optional limit")]
    async fn recall(&self, Parameters(p): Parameters<RecallParams>) -> String {
        let stack = MemoryStack::new(self.palace_path.clone(), self.db_path.clone());
        match stack
            .recall(p.wing.as_deref(), p.room.as_deref(), p.limit.unwrap_or(20) as usize)
            .await
        {
            Ok(drawers) => serde_json::to_string(&drawers)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 9: build_graph ──────────────────────────────────────────────────

    #[tool(description = "Build the palace graph: rooms as nodes, shared wings as edges")]
    async fn build_graph(&self) -> String {
        let pg = PalaceGraph { db_path: self.db_path.clone() };
        match pg.build_graph().await {
            Ok(nodes) => serde_json::to_string(&nodes)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 10: traverse_graph ──────────────────────────────────────────────

    #[tool(description = "BFS traverse from a start room up to max_hops away")]
    async fn traverse_graph(&self, Parameters(p): Parameters<TraverseGraphParams>) -> String {
        let pg = PalaceGraph { db_path: self.db_path.clone() };
        match pg
            .traverse_graph(&p.start_room, p.max_hops.unwrap_or(3) as usize)
            .await
        {
            Ok(nodes) => serde_json::to_string(&nodes)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 11: find_tunnels ────────────────────────────────────────────────

    #[tool(description = "Find rooms shared across multiple wings (tunnels)")]
    async fn find_tunnels(&self, Parameters(p): Parameters<FindTunnelsParams>) -> String {
        let pg = PalaceGraph { db_path: self.db_path.clone() };
        match pg
            .find_tunnels(p.wing_a.as_deref(), p.wing_b.as_deref())
            .await
        {
            Ok(nodes) => serde_json::to_string(&nodes)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 12: palace_graph_stats ──────────────────────────────────────────

    #[tool(description = "Statistics about the palace graph topology")]
    async fn palace_graph_stats(&self) -> String {
        let pg = PalaceGraph { db_path: self.db_path.clone() };
        match pg.stats().await {
            Ok(v) => v.to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 13: add_drawer ──────────────────────────────────────────────────

    #[tool(description = "Add a memory drawer to the palace")]
    async fn add_drawer(&self, Parameters(p): Parameters<AddDrawerParams>) -> String {
        let vec = match self.embed(&p.content).await {
            Ok(v) => v,
            Err(e) => return serde_json::json!({ "error": e.to_string() }).to_string(),
        };
        let now = chrono::Utc::now().to_rfc3339();
        let id = format!(
            "drawer_{}_{}_{}",
            p.wing,
            p.room,
            chrono::Utc::now().timestamp_millis()
        );
        let drawer = Drawer {
            id: id.clone(),
            content: p.content,
            wing: p.wing,
            room: p.room,
            source_file: "mcp".to_string(),
            source_mtime: 0,
            chunk_index: 0,
            added_by: p.agent.clone().unwrap_or_else(|| "mcp".to_string()),
            filed_at: now,
            hall: p.hall.unwrap_or_default(),
            topic: p.topic.unwrap_or_default(),
            drawer_type: p.drawer_type.unwrap_or_default(),
            agent: p.agent.unwrap_or_default(),
            date: p.date.unwrap_or_default(),
            importance: p.importance.unwrap_or(3.0),
        };
        let db_path = self.db_path.clone();
        let drawer_clone = drawer.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.add_drawer(&drawer_clone, &vec)?;
            Ok(drawer_clone.id)
        })
        .await;
        match result {
            Ok(Ok(rid)) => serde_json::json!({ "success": true, "id": rid }).to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 14: delete_drawer ───────────────────────────────────────────────

    #[tool(description = "Delete a drawer by ID")]
    async fn delete_drawer(&self, Parameters(p): Parameters<DeleteDrawerParams>) -> String {
        let db_path = self.db_path.clone();
        let id = p.id.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.delete_drawer(&id)
        })
        .await;
        match result {
            Ok(Ok(n)) => serde_json::json!({ "deleted": n > 0, "rows_affected": n }).to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 15: kg_query ────────────────────────────────────────────────────

    #[tool(description = "Query knowledge graph triples by entity name")]
    async fn kg_query(&self, Parameters(p): Parameters<KgQueryParams>) -> String {
        let kg_path = self.kg_path.clone();
        let entity = p.entity.clone();
        let direction = p.direction.clone().unwrap_or_else(|| "both".to_string());
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            KnowledgeGraph::new(&kg_path)?.query_entity(&entity, &direction)
        })
        .await;
        match result {
            Ok(Ok(triples)) => serde_json::to_string(&triples)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 16: kg_add ──────────────────────────────────────────────────────

    #[tool(description = "Add a knowledge graph triple (subject, predicate, object)")]
    async fn kg_add(&self, Parameters(p): Parameters<KgAddParams>) -> String {
        let kg_path = self.kg_path.clone();
        let subject = p.subject.clone();
        let predicate = p.predicate.clone();
        let object = p.object.clone();
        let confidence = p.confidence.unwrap_or(1.0);
        let source_closet = p.source_closet.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            KnowledgeGraph::new(&kg_path)?.add_triple(
                &subject,
                &predicate,
                &object,
                confidence,
                source_closet.as_deref(),
                None,
            )
        })
        .await;
        match result {
            Ok(Ok(id)) => serde_json::json!({ "success": true, "triple_id": id }).to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 17: kg_invalidate ───────────────────────────────────────────────

    #[tool(description = "Invalidate (soft-delete) a knowledge graph triple")]
    async fn kg_invalidate(&self, Parameters(p): Parameters<KgInvalidateParams>) -> String {
        let kg_path = self.kg_path.clone();
        let subject = p.subject.clone();
        let predicate = p.predicate.clone();
        let object = p.object.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            KnowledgeGraph::new(&kg_path)?.invalidate_triple(&subject, &predicate, &object)
        })
        .await;
        match result {
            Ok(Ok(n)) => serde_json::json!({ "invalidated": n }).to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 18: kg_timeline ─────────────────────────────────────────────────

    #[tool(description = "Get chronological timeline of triples for an entity")]
    async fn kg_timeline(&self, Parameters(p): Parameters<KgTimelineParams>) -> String {
        let kg_path = self.kg_path.clone();
        let entity = p.entity.clone().unwrap_or_default();
        let limit = p.limit.unwrap_or(50) as usize;
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let kg = KnowledgeGraph::new(&kg_path)?;
            if entity.is_empty() {
                kg.all_triples(limit)
            } else {
                kg.timeline(&entity, limit)
            }
        })
        .await;
        match result {
            Ok(Ok(triples)) => serde_json::to_string(&triples)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 19: kg_stats ────────────────────────────────────────────────────

    #[tool(description = "Statistics about the knowledge graph")]
    async fn kg_stats(&self) -> String {
        let kg_path = self.kg_path.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            KnowledgeGraph::new(&kg_path)?.stats()
        })
        .await;
        match result {
            Ok(Ok(v)) => v.to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 20: diary_write ─────────────────────────────────────────────────

    #[tool(description = "Write a diary entry for an agent")]
    async fn diary_write(&self, Parameters(p): Parameters<DiaryWriteParams>) -> String {
        let vec = match self.embed(&p.entry).await {
            Ok(v) => v,
            Err(e) => return serde_json::json!({ "error": e.to_string() }).to_string(),
        };
        let now = chrono::Utc::now();
        let id = format!("diary_{}_{}",p.agent_name, now.timestamp_millis());
        let drawer = Drawer {
            id: id.clone(),
            content: p.entry.clone(),
            wing: format!("diary_{}", p.agent_name),
            room: "entries".to_string(),
            source_file: "mcp".to_string(),
            source_mtime: 0,
            chunk_index: 0,
            added_by: p.agent_name.clone(),
            filed_at: now.to_rfc3339(),
            hall: String::new(),
            topic: p.topic.unwrap_or_default(),
            drawer_type: "diary".to_string(),
            agent: p.agent_name.clone(),
            date: now.format("%Y-%m-%d").to_string(),
            importance: 3.0,
        };
        let db_path = self.db_path.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.add_drawer(&drawer, &vec)?;
            Ok(drawer.id)
        })
        .await;
        match result {
            Ok(Ok(rid)) => serde_json::json!({ "success": true, "id": rid }).to_string(),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 21: diary_read ──────────────────────────────────────────────────

    #[tool(description = "Read diary entries for an agent")]
    async fn diary_read(&self, Parameters(p): Parameters<DiaryReadParams>) -> String {
        let db_path = self.db_path.clone();
        let wing = format!("diary_{}", p.agent_name);
        let limit = p.limit.unwrap_or(20) as usize;
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            VectorStorage::new(&db_path)?.get_all(Some(&wing), Some("entries"), limit)
        })
        .await;
        match result {
            Ok(Ok(drawers)) => serde_json::to_string(&drawers)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }).to_string()),
            Ok(Err(e)) => serde_json::json!({ "error": e.to_string() }).to_string(),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    // ── tool 22: get_aaak_spec ───────────────────────────────────────────────

    #[tool(description = "Return the AAAK dialect specification used for memory compression")]
    async fn get_aaak_spec(&self) -> String {
        serde_json::json!({ "spec": AAAK_SPEC }).to_string()
    }
}

#[tool_handler]
impl ServerHandler for MemPalace {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
    }
}

pub async fn run() -> anyhow::Result<()> {
    let server = MemPalace::new().await?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
