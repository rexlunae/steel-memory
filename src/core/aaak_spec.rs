pub const AAAK_SPEC: &str = r#"AAAK (Asynchronous AI Abbreviated Knowledge) Dialect Specification v1.0

FORMAT:
  WING|ROOM|DATE|FILE
  ENTITIES|TOPICS|"KEY_SENTENCE"|EMOTIONS|FLAGS

FIELDS:
  WING     - Memory wing (namespace)
  ROOM     - Memory room (category)
  DATE     - Date of memory (? if unknown)
  FILE     - Source file (mcp if added via MCP)
  ENTITIES - Key entities, joined by underscore (??? if none)
  TOPICS   - Topics from metadata (??? if none)
  KEY_SENTENCE - Most important sentence (quoted)
  EMOTIONS - Emotion codes: joy|sad|frus|conc|surp|resolve
  FLAGS    - Flag codes: CRIT|TODO|BUG|MIL|DEC

EXAMPLE:
  wing_proj|arch|2024-01-15|design.md
  postgresql_database|"decided to switch to PostgreSQL"|resolve|DEC

COMPRESSION RULES:
  - Stop words removed from entity extraction
  - Emotion detected from content keywords
  - Flags detected from content keywords
  - Key sentence = first sentence or first 80 chars
"#;
