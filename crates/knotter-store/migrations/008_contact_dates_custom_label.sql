-- 008_contact_dates_custom_label.sql

CREATE TRIGGER IF NOT EXISTS contact_dates_custom_label_insert
BEFORE INSERT ON contact_dates
WHEN NEW.kind = 'custom' AND length(trim(NEW.label)) = 0
BEGIN
  SELECT RAISE(ABORT, 'custom date label required');
END;

CREATE TRIGGER IF NOT EXISTS contact_dates_custom_label_update
BEFORE UPDATE ON contact_dates
WHEN NEW.kind = 'custom' AND length(trim(NEW.label)) = 0
BEGIN
  SELECT RAISE(ABORT, 'custom date label required');
END;
