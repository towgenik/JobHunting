-- profile_unlocked_files: comma-separated list of .md files that are NOT locked
-- in the profile editor. Empty = all files locked (default, since master CV is
-- AI-generated and manual edits may be overwritten).
ALTER TABLE settings ADD COLUMN profile_unlocked_files TEXT NOT NULL DEFAULT '';
