-- Drop orphaned columns never referenced in application code
ALTER TABLE settings DROP COLUMN interview_transcript;
ALTER TABLE settings DROP COLUMN chat_sessions;
