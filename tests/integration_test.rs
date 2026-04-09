use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_paths() {
        let config = steel_memory_lib::config_default_with_base(PathBuf::from("/tmp/test-palace"));
        assert!(config.vector_db.to_string_lossy().contains("palace.sqlite3"));
        assert!(config.kg_db.to_string_lossy().contains("knowledge_graph.sqlite3"));
    }

    #[test]
    fn test_cosine_similarity_identical() {
        use steel_memory_lib::cosine_similarity;
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6, "identical vectors should have similarity 1.0");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        use steel_memory_lib::cosine_similarity;
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "orthogonal vectors should have similarity 0.0");
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        use steel_memory_lib::cosine_similarity;
        let a = vec![0.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "zero vector should return 0.0");
    }

    #[test]
    fn test_normalize_entity_id() {
        use steel_memory_lib::normalize_id;
        assert_eq!(normalize_id("Hello World"), "hello_world");
        assert_eq!(normalize_id("foo--bar"), "foo_bar");
        assert_eq!(normalize_id("  spaces  "), "spaces");
        assert_eq!(normalize_id("CamelCase"), "camelcase");
    }

    #[test]
    fn test_aaak_compression() {
        use steel_memory_lib::{compress_to_aaak, Drawer};
        let drawer = Drawer {
            id: "test_id".to_string(),
            content: "We decided to switch to PostgreSQL because of performance issues.".to_string(),
            wing: "project".to_string(),
            room: "architecture".to_string(),
            source_file: "mcp".to_string(),
            source_mtime: 0,
            chunk_index: 0,
            added_by: "agent".to_string(),
            filed_at: "2024-01-15T00:00:00Z".to_string(),
            hall: String::new(),
            topic: "database".to_string(),
            drawer_type: String::new(),
            agent: "test_agent".to_string(),
            date: "2024-01-15".to_string(),
            importance: 4.0,
        };
        let aaak = compress_to_aaak(&drawer);
        assert!(aaak.contains("project|architecture"));
        assert!(aaak.contains("database"));
        // "decided" triggers resolve emotion, "issues" triggers conc
        assert!(aaak.contains("resolve") || aaak.contains("conc") || aaak.contains("DEC"));
    }

    #[test]
    fn test_vector_storage_add_get_delete() {
        use steel_memory_lib::{Drawer, storage::vector::VectorStorage};
        let tmp = tempdir();
        let db = tmp.join("palace.sqlite3");
        let storage = VectorStorage::new(&db).expect("create storage");

        let drawer = Drawer {
            id: "d_wing_room_1".to_string(),
            content: "Test memory content".to_string(),
            wing: "wing_test".to_string(),
            room: "room_test".to_string(),
            source_file: "mcp".to_string(),
            source_mtime: 0,
            chunk_index: 0,
            added_by: "test".to_string(),
            filed_at: "2024-01-01T00:00:00Z".to_string(),
            hall: String::new(),
            topic: "testing".to_string(),
            drawer_type: String::new(),
            agent: String::new(),
            date: "2024-01-01".to_string(),
            importance: 3.0,
        };
        let vec = vec![1.0f32, 0.0, 0.0];
        storage.add_drawer(&drawer, &vec).expect("add drawer");

        // Count
        assert_eq!(storage.count().expect("count"), 1);

        // Wings
        let wings = storage.list_wings().expect("list wings");
        assert_eq!(wings.len(), 1);
        assert_eq!(wings[0].0, "wing_test");
        assert_eq!(wings[0].1, 1);

        // Rooms
        let rooms = storage.list_rooms(None).expect("list rooms");
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].1, "room_test");

        // Taxonomy
        let tax = storage.get_taxonomy().expect("taxonomy");
        assert!(tax["wing_test"]["room_test"].as_u64().unwrap() == 1);

        // Get all
        let all = storage.get_all(Some("wing_test"), None, 10).expect("get all");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].content, "Test memory content");

        // Search
        let results = storage.search(&[1.0f32, 0.0, 0.0], 5, None, None).expect("search");
        assert!(!results.is_empty());
        assert!((results[0].similarity - 1.0).abs() < 1e-5);

        // Delete
        let deleted = storage.delete_drawer("d_wing_room_1").expect("delete");
        assert_eq!(deleted, 1);
        assert_eq!(storage.count().expect("count after delete"), 0);
    }

    #[test]
    fn test_vector_storage_search_filters() {
        use steel_memory_lib::{Drawer, storage::vector::VectorStorage};
        let tmp = tempdir();
        let db = tmp.join("palace.sqlite3");
        let storage = VectorStorage::new(&db).expect("create storage");

        for i in 0..3 {
            let d = Drawer {
                id: format!("d_{}", i),
                content: format!("Memory {}", i),
                wing: if i == 2 { "wing_other".to_string() } else { "wing_main".to_string() },
                room: if i == 1 { "room_b".to_string() } else { "room_a".to_string() },
                source_file: "mcp".to_string(),
                source_mtime: 0,
                chunk_index: 0,
                added_by: "test".to_string(),
                filed_at: "2024-01-01T00:00:00Z".to_string(),
                hall: String::new(),
                topic: String::new(),
                drawer_type: String::new(),
                agent: String::new(),
                date: String::new(),
                importance: 3.0,
            };
            storage.add_drawer(&d, &vec![1.0f32, 0.0, 0.0]).expect("add");
        }

        // Filter by wing
        let wing_results = storage.search(&[1.0f32, 0.0, 0.0], 10, Some("wing_main"), None).expect("search");
        assert_eq!(wing_results.len(), 2);

        // Filter by room
        let room_results = storage.search(&[1.0f32, 0.0, 0.0], 10, None, Some("room_a")).expect("search");
        assert_eq!(room_results.len(), 2);

        // Filter by both
        let both = storage.search(&[1.0f32, 0.0, 0.0], 10, Some("wing_main"), Some("room_a")).expect("search");
        assert_eq!(both.len(), 1);
    }

    #[test]
    fn test_knowledge_graph() {
        use steel_memory_lib::storage::knowledge_graph::KnowledgeGraph;
        let tmp = tempdir();
        let db = tmp.join("kg.sqlite3");
        let kg = KnowledgeGraph::new(&db).expect("create KG");

        // Add triple
        let id = kg.add_triple("Alice", "knows", "Bob", 1.0, None, None).expect("add triple");
        assert!(!id.is_empty());

        // Query outgoing
        let triples = kg.query_entity("Alice", "outgoing").expect("query");
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0].predicate, "knows");

        // Query incoming (Bob)
        let triples = kg.query_entity("Bob", "incoming").expect("query incoming");
        assert_eq!(triples.len(), 1);

        // Query both
        let triples = kg.query_entity("Alice", "both").expect("query both");
        assert_eq!(triples.len(), 1);

        // Stats
        let stats = kg.stats().expect("stats");
        assert_eq!(stats["entities"].as_i64().unwrap(), 2);
        assert_eq!(stats["triples"].as_i64().unwrap(), 1);

        // Invalidate
        let n = kg.invalidate_triple("Alice", "knows", "Bob").expect("invalidate");
        assert_eq!(n, 1);

        // After invalidation, active query returns nothing
        let triples = kg.query_entity("Alice", "outgoing").expect("query after invalidate");
        assert_eq!(triples.len(), 0);
    }

    #[test]
    fn test_knowledge_graph_timeline() {
        use steel_memory_lib::storage::knowledge_graph::KnowledgeGraph;
        let tmp = tempdir();
        let db = tmp.join("kg.sqlite3");
        let kg = KnowledgeGraph::new(&db).expect("create KG");

        kg.add_triple("Alice", "works_at", "Corp", 1.0, None, None).expect("add");
        kg.add_triple("Bob", "reports_to", "Alice", 0.9, None, None).expect("add");

        let timeline = kg.timeline("Alice", 10).expect("timeline");
        assert!(!timeline.is_empty());

        let all = kg.all_triples(10).expect("all triples");
        assert_eq!(all.len(), 2);
    }

    /// Helper: create a temp dir path (no cleanup needed for tests)
    fn tempdir() -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = PathBuf::from(format!("/tmp/steel_memory_test_{}", id));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
