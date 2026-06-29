-- Live progress messages shown in the polling card during the pipeline.
-- Written at each step (writer, reviewer, verifier, editor, ranker).
ALTER TABLE jobs ADD COLUMN progress TEXT NOT NULL DEFAULT '';
