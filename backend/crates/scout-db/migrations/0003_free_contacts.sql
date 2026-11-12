ALTER TABLE contacts ADD COLUMN is_generic BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE contacts DROP CONSTRAINT contacts_provenance_check;
ALTER TABLE contacts ADD CONSTRAINT contacts_provenance_check CHECK (provenance = 'page');

ALTER TABLE companies ADD COLUMN contacts_checked_at TIMESTAMPTZ;
