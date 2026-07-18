-- toolbar_actions was declared in 0006 and seeded, but no core or Tauri code ever
-- queries it (no SELECT/INSERT/UPDATE/DELETE). The action bar is built in the UI.
-- Drop the dead table so the schema matches actual usage.
DROP TABLE IF EXISTS toolbar_actions;
