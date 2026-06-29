-- Agent settings: configurable context window, max output, thinking effort
ALTER TABLE settings ADD COLUMN agent_ctx_window INTEGER DEFAULT 200000;
ALTER TABLE settings ADD COLUMN agent_max_output INTEGER DEFAULT 16384;
ALTER TABLE settings ADD COLUMN agent_thinking_effort TEXT DEFAULT 'high';
ALTER TABLE settings ADD COLUMN agent_wiki_query_max_hops INTEGER DEFAULT 10;
