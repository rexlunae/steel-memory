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
}
