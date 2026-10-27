use super::*;
use sqlite::{Connection, State};
use std::path::Path;

pub struct RecordingStore { pub(crate) connection: Connection }
impl RecordingStore {
    pub fn open(path: &Path) -> Result<Self, RecordingError> {
        let mut connection = Connection::open(path)?;
        connection.set_busy_timeout(2000)?;
        connection.execute("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        let version = connection.prepare("PRAGMA user_version")?.into_iter().next().transpose()?.map(|r| r.read::<i64, _>(0)).unwrap_or(0);
        if version > 2 { return Err(RecordingError::new("schema_version", "This diagnostics database requires a newer application.")); }
        if version == 0 { connection.execute(include_str!("migrations/001_recordings.sql"))?; }
        Ok(Self { connection })
    }
    pub fn insert_session(&mut self, session: &SessionMetadata) -> Result<(), RecordingError> {
        let mut s = self.connection.prepare("INSERT INTO recording_sessions(id,vehicle_id,started_utc,metadata) VALUES(?,?,?,?)")?;
        s.bind((1,session.id.as_str()))?; s.bind((2,session.vehicle_id.as_str()))?; s.bind((3,session.started_at.utc_ms))?; s.bind((4,serde_json::to_string(session)?.as_str()))?; s.next()?; Ok(())
    }
    pub fn session(&self, id: &str) -> Result<SessionMetadata, RecordingError> {
        let mut s = self.connection.prepare("SELECT metadata FROM recording_sessions WHERE id=?")?; s.bind((1,id))?;
        if s.next()? != State::Row { return Err(RecordingError::new("not_found", "Recording no longer exists.")); }
        Ok(serde_json::from_str(&s.read::<String,_>(0)?)?)
    }
    pub(crate) fn save_metadata(&self, session: &SessionMetadata) -> Result<(), RecordingError> {
        let mut s = self.connection.prepare("UPDATE recording_sessions SET metadata=?, vehicle_id=?, revision=revision+1 WHERE id=?")?;
        s.bind((1,serde_json::to_string(session)?.as_str()))?; s.bind((2,session.vehicle_id.as_str()))?; s.bind((3,session.id.as_str()))?; s.next()?; Ok(())
    }
    pub fn append_snapshot(&mut self, id: &str, captured: Timestamp, values: &[Measurement]) -> Result<(), RecordingError> {
        let mut session = self.session(id)?;
        if session.ended_at.is_some() { return Err(RecordingError::new("stopped", "Recording has already stopped.")); }
        if values.iter().any(|v| v.generation != session.generation) { return Err(RecordingError::new("stale_connection", "Reading connection does not match recording.")); }
        let elapsed = captured.elapsed_ms.saturating_sub(session.started_at.elapsed_ms);
        self.connection.execute("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<(), RecordingError> {
            let mut s = self.connection.prepare("INSERT INTO snapshots(session_id,elapsed_ms,captured_utc) VALUES(?,?,?)")?;
            s.bind((1,id))?; s.bind((2,elapsed as i64))?; s.bind((3,captured.utc_ms))?; s.next()?;
            let mut last_id = self.connection.prepare("SELECT last_insert_rowid()")?;
            last_id.next()?;
            let snapshot_id = last_id.read::<i64,_>(0)?;
            for measurement in values {
                let mut previous = self.connection.prepare("SELECT payload FROM measurements WHERE session_id=? AND metric_id=? ORDER BY id DESC LIMIT 1")?;
                previous.bind((1,id))?; previous.bind((2,measurement.metric_id.as_str()))?;
                let repeated = if previous.next()? == State::Row { serde_json::from_str::<SampleRow>(&previous.read::<String,_>(0)?)?.measurement.observation_id == measurement.observation_id } else { false };
                let age = captured.elapsed_ms.saturating_sub(measurement.acquired_at.elapsed_ms);
                let row = SampleRow { row_id: 0, session_id:id.into(), elapsed_ms:elapsed, captured_at:captured.clone(), age_ms:age, stale:age>30_000, pre_start:measurement.acquired_at.elapsed_ms<session.started_at.elapsed_ms, repeated, measurement:measurement.clone() };
                let mut s = self.connection.prepare("INSERT INTO measurements(session_id,snapshot_id,metric_id,elapsed_ms,payload) VALUES(?,?,?,?,?)")?;
                s.bind((1,id))?; s.bind((2,snapshot_id))?; s.bind((3,measurement.metric_id.as_str()))?; s.bind((4,elapsed as i64))?; s.bind((5,serde_json::to_string(&row)?.as_str()))?; s.next()?;
            }
            session.snapshot_count+=1; self.save_metadata(&session)?; Ok(())
        })();
        match result { Ok(()) => { self.connection.execute("COMMIT")?; Ok(()) }, Err(e) => { let _=self.connection.execute("ROLLBACK"); Err(e) } }
    }
    pub fn finalize(&mut self, id: &str, reason: &str, ended: Timestamp) -> Result<(), RecordingError> {
        let mut session = self.session(id)?; session.ended_at=Some(ended); session.reason=Some(reason.into()); self.save_metadata(&session)
    }
    pub fn list_sessions(&self, limit: usize, offset: usize) -> Result<Vec<SessionMetadata>, RecordingError> {
        let mut s=self.connection.prepare("SELECT metadata FROM recording_sessions ORDER BY started_utc DESC,id LIMIT ? OFFSET ?")?;
        s.bind((1,limit.min(100) as i64))?; s.bind((2,offset as i64))?;
        let mut out=Vec::new(); while s.next()? == State::Row { out.push(serde_json::from_str(&s.read::<String,_>(0)?)?); } Ok(out)
    }
    pub fn samples(&self, id: &str, metrics: &[String], start: u64, end: u64, limit: usize, cursor: i64) -> Result<SamplePage, RecordingError> {
        self.session(id)?;
        if start>end || metrics.len()>10 || limit==0 || limit>500 || cursor<0 { return Err(RecordingError::new("invalid_input", "Use a valid time range, up to 10 metrics and 1–500 rows.")); }
        if cursor>0 {
            let mut s=self.connection.prepare("SELECT session_id FROM measurements WHERE id=?")?; s.bind((1,cursor))?;
            if s.next()?!=State::Row || s.read::<String,_>(0)?!=id { return Err(RecordingError::new("invalid_input", "Cursor does not belong to this recording.")); }
        }
        let mut s=self.connection.prepare("SELECT id,payload FROM measurements WHERE session_id=? AND elapsed_ms>=? AND elapsed_ms<? AND id>? ORDER BY id")?;
        s.bind((1,id))?; s.bind((2,start.min(i64::MAX as u64) as i64))?; s.bind((3,end.min(i64::MAX as u64) as i64))?; s.bind((4,cursor))?;
        let mut page=SamplePage { rows:Vec::new(), next_cursor:None, truncated:false }; let mut bytes=128;
        while s.next()?==State::Row {
            let mut row:SampleRow=serde_json::from_str(&s.read::<String,_>(1)?)?; row.row_id=s.read(0)?;
            if !metrics.is_empty() && !metrics.contains(&row.measurement.metric_id) { continue; }
            let row_bytes=serde_json::to_vec(&row)?.len()+2;
            if page.rows.len()>=limit || bytes+row_bytes>64*1024-256 { page.truncated=true; page.next_cursor=page.rows.last().map(|r|r.row_id); break; }
            bytes+=row_bytes; page.rows.push(row);
        }
        Ok(page)
    }
}
