-- Search/batch-crawl tracking.
-- `search_id` on jobs is nullable — individually pasted jobs have NULL.
ALTER TABLE jobs ADD COLUMN search_id TEXT;

CREATE TABLE searches (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    found_count INTEGER DEFAULT 0
);
