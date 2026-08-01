-- Report catalog: hide intermediate / infrastructure report types from the picker.

ALTER TABLE lore.report_types
    ADD COLUMN IF NOT EXISTS hidden INTEGER NOT NULL DEFAULT 0;

UPDATE lore.report_types SET hidden = 1
WHERE id IN (
    'chapter_summaries',
    'genre_ranking',
    'genre_analysis',
    'kdp_categories',
    'kdp_keywords',
    'bisac_classification',
    'discovery_keywords',
    'google_keyword_search',
    'content_maturity_advisory',
    'wide_metadata_paste'
);
