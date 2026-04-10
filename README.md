# steel-memory

A Rust implementation of [mempalace](https://github.com/adshaa/mempalacejs) — a spatial memory palace for AI agents. Rewritten in Rust for size, speed, and efficiency.

## Features

- **22 MCP tools** matching the mempalacejs API
- **Semantic vector search** using [FastEmbed](https://github.com/Anush008/fastembed-rs) (AllMiniLML6V2, ~90MB, downloaded on first use)
- **SQLite storage** — no external database needed (`~/.steel-memory/palace.sqlite3`)
- **Knowledge graph** — temporal RDF-style triples with invalidation (`knowledge_graph.sqlite3`)
- **Palace graph** — BFS traversal across rooms/wings, tunnel detection
- **AAAK dialect** — compressed memory format for efficient context priming
- **4-layer memory stack** — L0 identity, L1 AAAK story, L2 on-demand recall, L3 semantic search
- **Agent diary** — per-agent timestamped journal

## Installation

```bash
cargo build --release
```

Binary: `target/release/steel-memory`

## MCP Server

Communicates over stdin/stdout using the [Model Context Protocol](https://modelcontextprotocol.io/) JSON-RPC protocol.

### Claude Desktop / MCP Config

```json
{
  "mcpServers": {
    "steel-memory": {
      "command": "/path/to/steel-memory"
    }
  }
}
```

## Tools

| Tool | Description |
|---|---|
| `mempalace_status` | Total drawers and palace path |
| `mempalace_list_wings` | All wings with counts |
| `mempalace_list_rooms` | All rooms (optional wing filter) |
| `mempalace_get_taxonomy` | Full wing → room → count map |
| `mempalace_search` | Semantic search (query, limit?, wing?, room?) |
| `mempalace_check_duplicate` | Duplicate detection by similarity |
| `mempalace_wake_up` | L0 identity + L1 AAAK context (wing?) |
| `mempalace_recall` | L2 on-demand drawer list (wing?, room?, limit?) |
| `mempalace_build_graph` | Palace room/wing graph |
| `mempalace_traverse_graph` | BFS from a room (start_room, max_hops?) |
| `mempalace_find_tunnels` | Rooms shared across wings |
| `mempalace_graph_stats` | Graph topology statistics |
| `mempalace_add_drawer` | Add memory (wing, room, content) |
| `mempalace_delete_drawer` | Delete memory by ID |
| `mempalace_kg_query` | Query KG by entity (direction?) |
| `mempalace_kg_add` | Add triple (subject, predicate, object) |
| `mempalace_kg_invalidate` | Soft-delete a triple |
| `mempalace_kg_timeline` | Chronological triples for an entity |
| `mempalace_kg_stats` | Knowledge graph statistics |
| `mempalace_diary_write` | Write agent diary entry |
| `mempalace_diary_read` | Read agent diary entries |
| `mempalace_get_aaak_spec` | AAAK dialect specification |

## Configuration

| Path | Description |
|---|---|
| `~/.steel-memory/` | Palace root directory |
| `~/.steel-memory/palace.sqlite3` | Vector + drawer storage |
| `~/.steel-memory/knowledge_graph.sqlite3` | Knowledge graph |
| `~/.steel-memory/identity.txt` | L0 identity (create manually) |

Set `STEEL_MEMORY_COLLECTION` env var to change the collection name.

## First Run

On first run with semantic search enabled, the AllMiniLML6V2 model (~90MB) will be downloaded automatically from Hugging Face. Subsequent runs use the cached model.

## Testing

```bash
cargo test
```

## LongMemEval Benchmarking

You can benchmark steel-memory retrieval against [LongMemEval](https://github.com/xiaowu0162/LongMemEval) with the included CLI.

1. Download one of the LongMemEval JSON files, such as `longmemeval_oracle.json`, `longmemeval_s_cleaned.json`, or `longmemeval_m_cleaned.json`.
2. Run the benchmark:

```bash
cargo bench --bench longmemeval -- \
  --data /absolute/path/to/longmemeval_oracle.json \
  --granularity session \
  --output /absolute/path/to/longmemeval-results.jsonl
```

If you prefer, the same benchmark can also be run with `cargo run --bin longmemeval-benchmark -- ...`.

Options:

- `--granularity session|turn` controls whether the benchmark indexes whole sessions or user turns
- `--max-questions N` limits the number of evaluation instances for a smaller test run
- `--output` writes per-question retrieval results as JSONL while the summary is printed to stdout

The benchmark uses steel-memory's embedding and SQLite-backed search stack, and reports LongMemEval-style retrieval metrics such as `recall_any@k`, `recall_all@k`, and `ndcg_any@k`. On first use, the embedding model may still need to be downloaded.

The benchmark uses the same FastEmbed `AllMiniLML6V2` model as the MCP server. The first run may download roughly 90MB of model data before benchmarking starts.
