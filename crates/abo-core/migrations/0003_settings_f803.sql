-- 0003_settings_f803.sql - v0.4.0 (seeing) Phase 2 (F-803 app settings),
-- additive over 0001_init.sql (which created the singleton `settings` row) and
-- 0002_plan_and_rulesets.sql (which added `scan_retention_count`). This is the
-- migration migration 0001's comment reserved: "F-803 (v0.4.0) adds
-- theme/roots/retention columns via additive migrations" (spec OQ-2). It only
-- ADDs columns to the existing singleton row; it never resets or drops, so a
-- v0.3.0 database upgrades in place with its data preserved.
--
-- Column set (architecture Section 4 settings DDL, spec F-803 / AC-34):
--   library_root     the sanctioned scan root the backend re-allows at startup
--                    (FD-29, F-909). NULL until first-run picks one, which is
--                    exactly how the shell detects first-run (a NULL root means
--                    no library is configured yet).
--   quarantine_root  where set-aside items land (the internal "quarantine"
--                    vocabulary is kept ONLY here on disk artifacts and code,
--                    per FD-31; the IPC/UI vocabulary is "set aside"). NULL =
--                    use the default beside the app data.
--   reports_root     an optional override for the Reports folder (F-1002).
--                    NULL = use the default beside the app data.
--   theme            'day' | 'evening' (FD-09). NOT NULL DEFAULT 'day' so the
--                    default-theme guarantee (AC-2, first-run Day default)
--                    holds for every row, including the pre-existing one this
--                    migration backfills.
--
-- Each ALTER is additive with either a NULLable column or a NOT NULL DEFAULT,
-- so it backfills the one existing `settings` row (id = 1) from 0001 without a
-- rewrite. `scan_retention_count` (FD-20, AC-35) already exists from 0002 and
-- is intentionally NOT re-added here.
ALTER TABLE settings ADD COLUMN library_root    TEXT;
ALTER TABLE settings ADD COLUMN quarantine_root TEXT;
ALTER TABLE settings ADD COLUMN reports_root    TEXT;
ALTER TABLE settings ADD COLUMN theme           TEXT NOT NULL DEFAULT 'day';
