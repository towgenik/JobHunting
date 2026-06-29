ALTER TABLE settings ADD COLUMN llm_endpoint TEXT;
ALTER TABLE settings ADD COLUMN llm_api_key TEXT;
ALTER TABLE settings ADD COLUMN llm_model TEXT;
ALTER TABLE settings ADD COLUMN llm_openai_compat INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN llm_mock INTEGER DEFAULT 0;
ALTER TABLE settings ADD COLUMN scheduler_interval_minutes INTEGER DEFAULT 0;
ALTER TABLE settings ADD COLUMN scheduler_date_range INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN scheduler_max_pages INTEGER DEFAULT 5;
