-- Per-agent overrides for pipeline LLM roles.
-- NULL columns mean "inherit the global default".
-- Roles: prescreen, writer, reviewer, verifier, editor, ranker
CREATE TABLE IF NOT EXISTS agent_overrides (
    role             TEXT PRIMARY KEY,
    max_output       INTEGER,
    thinking_effort  TEXT
);
