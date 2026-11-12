-- Roles already listed the first time a board is read are not "new": Scout cannot know
-- when they appeared. Only roles that show up on a later read are.
ALTER TABLE jobs ADD COLUMN baseline BOOLEAN NOT NULL DEFAULT false;

-- Existing rows: whatever each board listed on its first read (within that run) is baseline.
UPDATE jobs j SET baseline = true
FROM (SELECT board_id, min(first_seen_at) AS first_read FROM jobs GROUP BY board_id) b
WHERE j.board_id = b.board_id AND j.first_seen_at < b.first_read + interval '30 minutes';
