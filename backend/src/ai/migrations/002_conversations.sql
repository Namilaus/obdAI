BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS conversations (
 id TEXT PRIMARY KEY, metadata TEXT NOT NULL, vehicle_id TEXT,
 revision INTEGER NOT NULL DEFAULT 0, active_turn TEXT, status TEXT NOT NULL DEFAULT 'ready'
);
CREATE TABLE IF NOT EXISTS conversation_recordings (
 conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
 session_id TEXT NOT NULL REFERENCES recording_sessions(id) ON DELETE CASCADE,
 PRIMARY KEY(conversation_id,session_id)
);
CREATE INDEX IF NOT EXISTS conversation_recording_sessions ON conversation_recordings(session_id);
CREATE TABLE IF NOT EXISTS messages (
 id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
 turn_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, extra TEXT, status TEXT NOT NULL DEFAULT 'pending'
);
CREATE INDEX IF NOT EXISTS messages_conversation ON messages(conversation_id,id);
CREATE TABLE IF NOT EXISTS ai_preferences (id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);
PRAGMA user_version = 2;
COMMIT;
