-- Wiki sync: track last ingest time and auto-ingest flag
ALTER TABLE settings ADD COLUMN wiki_last_ingest_at INTEGER;
ALTER TABLE settings ADD COLUMN wiki_auto_ingest    INTEGER DEFAULT 0;
