PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS recording_sessions (
 id TEXT PRIMARY KEY, vehicle_id TEXT NOT NULL, started_utc INTEGER NOT NULL,
 metadata TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS snapshots (
 id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES recording_sessions(id) ON DELETE CASCADE,
 elapsed_ms INTEGER NOT NULL, captured_utc INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS measurements (
 id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES recording_sessions(id) ON DELETE CASCADE,
 snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
 metric_id TEXT NOT NULL, elapsed_ms INTEGER NOT NULL, payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS measurements_session_time ON measurements(session_id, elapsed_ms, id);
CREATE INDEX IF NOT EXISTS measurements_session_metric ON measurements(session_id, metric_id, elapsed_ms);
CREATE TABLE IF NOT EXISTS diagnostic_events (
 id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES recording_sessions(id) ON DELETE CASCADE,
 kind TEXT NOT NULL, acquired_utc INTEGER NOT NULL, payload TEXT NOT NULL
);
PRAGMA user_version = 1;
