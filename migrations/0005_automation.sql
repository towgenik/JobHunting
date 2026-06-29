-- Created timestamp for time-window queries (stale job recovery, archival)
-- Existing rows get NULL — use rowid for ordering those.
ALTER TABLE jobs ADD COLUMN created_at TEXT;

-- Review loop results
ALTER TABLE jobs ADD COLUMN review_score INTEGER;
ALTER TABLE jobs ADD COLUMN review_feedback TEXT;

-- Truth/lie verification blob (JSON: {truth_pct, items, gap_report})
ALTER TABLE jobs ADD COLUMN verification TEXT;

-- Ranker prediction blob (JSON: {approval_probability, good, bad, improvements})
ALTER TABLE jobs ADD COLUMN rank TEXT;

-- Human review text on approval/rejection.
-- Plain text. Writer/reviewer/ranker receive recent quality-related notes
-- as few-shot examples. QC-filtered at read time (skip non-quality reasons).
ALTER TABLE jobs ADD COLUMN review_notes TEXT;

-- Structured rejection reason category. One of: quality_gap, already_applied,
-- location, salary, company, other. Populated from DecisionForm.
-- Used to filter review_notes for few-shot (only quality_gap feeds learning).
ALTER TABLE jobs ADD COLUMN decision_reason TEXT;

-- LLM-generated + user-curated recurring search keywords.
CREATE TABLE search_queries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    query       TEXT UNIQUE NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT
);

-- Audit trail for scheduled runs.
CREATE TABLE scheduler_runs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at    TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at   TEXT,
    status        TEXT NOT NULL DEFAULT 'running',
    queries_run   INTEGER NOT NULL DEFAULT 0,
    jobs_found    INTEGER NOT NULL DEFAULT 0,
    jobs_filtered INTEGER NOT NULL DEFAULT 0,
    errors        TEXT
);
