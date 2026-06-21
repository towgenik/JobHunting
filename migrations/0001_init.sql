CREATE TABLE jobs (
    id TEXT PRIMARY KEY,                 -- uuid v4 as TEXT: Uuid::new_v4().to_string()
    url TEXT UNIQUE NOT NULL,
    title TEXT,
    description TEXT,
    cv TEXT,                             -- JSON CV the LLM returned (one per job)
    reject_reason TEXT,                  -- populated when user rejects; folded in at M1 to avoid a 2nd migration
    -- new | scraping | generating | pending_approval | approved | rejected | failed
    status TEXT DEFAULT 'new'
);

-- Single-row settings table; upsert on save
CREATE TABLE settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    master_cv TEXT NOT NULL DEFAULT ''
);
INSERT INTO settings (id, master_cv) VALUES (1, '');
