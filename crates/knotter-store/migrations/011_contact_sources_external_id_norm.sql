-- 011_contact_sources_external_id_norm.sql
-- Add normalized external IDs for case-insensitive matching.

ALTER TABLE contact_sources ADD COLUMN external_id_norm TEXT;

UPDATE contact_sources
  SET external_id_norm = lower(external_id);

WITH duplicate_norms AS (
  SELECT source, external_id_norm
    FROM contact_sources
   WHERE external_id_norm IS NOT NULL
   GROUP BY source, external_id_norm
  HAVING COUNT(*) > 1
)
UPDATE contact_sources
   SET external_id_norm = NULL
 WHERE EXISTS (
       SELECT 1
         FROM duplicate_norms
        WHERE duplicate_norms.source = contact_sources.source
          AND duplicate_norms.external_id_norm = contact_sources.external_id_norm
   );

CREATE UNIQUE INDEX IF NOT EXISTS idx_contact_sources_source_external_id_norm
  ON contact_sources(source, external_id_norm)
 WHERE external_id_norm IS NOT NULL;
