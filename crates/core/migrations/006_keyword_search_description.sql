-- Neutral user-facing copy for keyword search report (no vendor name).
UPDATE lore.report_types
SET description = 'Amazon keyword search volume and competition estimates.'
WHERE id = 'keyword_search';
