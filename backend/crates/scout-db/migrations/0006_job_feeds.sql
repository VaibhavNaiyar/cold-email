-- Job feeds list many companies on one board, so each role carries its own company.
ALTER TABLE jobs ADD COLUMN company TEXT;

ALTER TABLE job_boards DROP CONSTRAINT job_boards_ats_check;
ALTER TABLE job_boards ADD CONSTRAINT job_boards_ats_check
    CHECK (ats IN ('greenhouse', 'lever', 'ashby', 'yc', 'remoteok', 'himalayas', 'hn'));
