-- Whether the hiring company is a startup, as judged when the role is scored. Null: unknown.
ALTER TABLE jobs ADD COLUMN is_startup BOOLEAN;
-- Open roles are scored again so every one gets the startup judgement.
UPDATE jobs SET fit = NULL, fit_reason = NULL WHERE closed_at IS NULL;

-- Boards found by searching the hosts of careers boards, and the We Work Remotely feeds.
ALTER TABLE job_boards DROP CONSTRAINT job_boards_source_check;
ALTER TABLE job_boards ADD CONSTRAINT job_boards_source_check
    CHECK (source IN ('yc', 'website', 'manual', 'search'));

ALTER TABLE job_boards DROP CONSTRAINT job_boards_ats_check;
ALTER TABLE job_boards ADD CONSTRAINT job_boards_ats_check
    CHECK (ats IN ('greenhouse', 'lever', 'ashby', 'yc', 'remoteok', 'himalayas', 'hn', 'wwr'));
