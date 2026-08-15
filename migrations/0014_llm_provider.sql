-- Add provider column (openai | openai-compat | anthropic | google)
-- Backfill from legacy llm_openai_compat boolean.
ALTER TABLE settings ADD COLUMN llm_provider TEXT;

UPDATE settings
  SET llm_provider = CASE llm_openai_compat
    WHEN 0 THEN 'anthropic'
    ELSE 'openai'
  END
  WHERE llm_provider IS NULL;

-- Raise agent defaults to match Rust Default impls
UPDATE settings
  SET agent_ctx_window = 1048576
  WHERE agent_ctx_window IS NULL OR agent_ctx_window < 1048576;

UPDATE settings
  SET agent_max_output = 131072
  WHERE agent_max_output IS NULL OR agent_max_output < 131072;
