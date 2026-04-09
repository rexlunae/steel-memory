use std::path::PathBuf;
use std::collections::{HashMap, HashSet, VecDeque};
use rusqlite::Connection;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoomNode {
    pub id: String,
    pub wing: String,
    pub room: String,
    pub count: usize,
}

pub struct PalaceGraph {
    pub db_path: PathBuf,
}

impl PalaceGraph {
    pub async fn build_graph(&self) -> anyhow::Result<Vec<RoomNode>> {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_conn(&db_path)?;
            let mut stmt = conn.prepare(
                "SELECT wing, room, COUNT(*) as cnt FROM drawers GROUP BY wing, room ORDER BY wing, room"
            )?;
            let nodes = stmt.query_map([], |row| {
                let wing: String = row.get(0)?;
                let room: String = row.get(1)?;
                let count: i64 = row.get(2)?;
                Ok(RoomNode {
                    id: format!("{}/{}", wing, room),
                    wing,
                    room,
                    count: count as usize,
                })
            })?.filter_map(|r| r.ok()).collect();
            Ok(nodes)
        }).await?
    }

    pub async fn traverse_graph(
        &self,
        start_room: &str,
        max_hops: usize,
    ) -> anyhow::Result<Vec<RoomNode>> {
        let all_nodes = self.build_graph().await?;
        let start = start_room.to_string();

        // Build wing -> rooms map and room -> wings map
        let mut wing_to_rooms: HashMap<String, Vec<String>> = HashMap::new();
        let mut room_to_wings: HashMap<String, Vec<String>> = HashMap::new();
        let mut node_map: HashMap<String, RoomNode> = HashMap::new();

        for node in &all_nodes {
            wing_to_rooms.entry(node.wing.clone()).or_default().push(node.room.clone());
            room_to_wings.entry(node.room.clone()).or_default().push(node.wing.clone());
            node_map.insert(node.id.clone(), node.clone());
        }

        // Determine start node id
        let start_id = if start.contains('/') {
            start.clone()
        } else {
            // Find any node whose room matches
            all_nodes.iter()
                .find(|n| n.room == start)
                .map(|n| n.id.clone())
                .unwrap_or(start.clone())
        };

        // BFS
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut result: Vec<RoomNode> = Vec::new();

        queue.push_back((start_id.clone(), 0));
        visited.insert(start_id.clone());

        while let Some((current_id, hops)) = queue.pop_front() {
            if let Some(node) = node_map.get(&current_id) {
                result.push(node.clone());
                if hops < max_hops {
                    // Find connected rooms via shared wing
                    if let Some(rooms) = wing_to_rooms.get(&node.wing) {
                        for r in rooms {
                            let neighbor_id = format!("{}/{}", node.wing, r);
                            if !visited.contains(&neighbor_id) {
                                visited.insert(neighbor_id.clone());
                                queue.push_back((neighbor_id, hops + 1));
                            }
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    pub async fn find_tunnels(
        &self,
        wing_a: Option<&str>,
        wing_b: Option<&str>,
    ) -> anyhow::Result<Vec<RoomNode>> {
        let all_nodes = self.build_graph().await?;

        // Find rooms that appear in 2+ wings
        let mut room_to_wings: HashMap<String, Vec<String>> = HashMap::new();
        for node in &all_nodes {
            room_to_wings.entry(node.room.clone()).or_default().push(node.wing.clone());
        }

        let tunnels: Vec<RoomNode> = all_nodes.into_iter().filter(|node| {
            let wings = room_to_wings.get(&node.room).map(|v| v.len()).unwrap_or(0);
            if wings < 2 { return false; }
            match (wing_a, wing_b) {
                (Some(wa), Some(wb)) => {
                    let ws = room_to_wings.get(&node.room).unwrap();
                    ws.contains(&wa.to_string()) && ws.contains(&wb.to_string())
                }
                (Some(wa), None) => node.wing == wa,
                (None, Some(wb)) => node.wing == wb,
                (None, None) => true,
            }
        }).collect();
        Ok(tunnels)
    }

    pub async fn stats(&self) -> anyhow::Result<serde_json::Value> {
        let nodes = self.build_graph().await?;
        let wings: HashSet<String> = nodes.iter().map(|n| n.wing.clone()).collect();
        let rooms: HashSet<String> = nodes.iter().map(|n| n.room.clone()).collect();
        let total_drawers: usize = nodes.iter().map(|n| n.count).sum();

        // Find tunnels (rooms in 2+ wings)
        let mut room_wing_count: HashMap<String, usize> = HashMap::new();
        for n in &nodes {
            *room_wing_count.entry(n.room.clone()).or_insert(0) += 1;
        }
        let tunnel_count = room_wing_count.values().filter(|&&c| c >= 2).count();

        Ok(serde_json::json!({
            "wings": wings.len(),
            "rooms": rooms.len(),
            "nodes": nodes.len(),
            "total_drawers": total_drawers,
            "tunnels": tunnel_count,
        }))
    }
}

fn open_conn(path: &std::path::Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    Ok(conn)
}
