-- Chat transcript for the CV-builder assistant, stored alongside the master CV
-- in the single-row settings table. JSON array of {"role","content"}.
ALTER TABLE settings ADD COLUMN interview_transcript TEXT NOT NULL DEFAULT '[]';
