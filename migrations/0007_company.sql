-- Company name for each job listing. The scraper extracts it from the HTML
-- (data-automation="advertiser-name") and the index API already returns it.
ALTER TABLE jobs ADD COLUMN company TEXT NOT NULL DEFAULT '';
