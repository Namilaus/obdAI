use serde::{Deserialize, Serialize};
use std::time::Instant;
use once_cell::sync::Lazy;

pub static MONOTONIC_ORIGIN: Lazy<Instant> = Lazy::new(Instant::now);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Timestamp { pub utc_ms: i64, pub elapsed_ms: u64 }
impl Timestamp {
    pub fn now() -> Self { Self { utc_ms: chrono::Utc::now().timestamp_millis(), elapsed_ms: MONOTONIC_ORIGIN.elapsed().as_millis() as u64 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode { Live, Demo, Replay, FreezeFrame }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability { Valid, NoData, Error }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionContext { pub generation: u64, pub vehicle_id: String, pub source: SourceMode, pub simulated: bool }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub recording: bool,
    pub active_session: Option<String>,
    pub snapshot_count: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub metric_id: String,
    pub name: String,
    pub observation_id: u64,
    pub generation: u64,
    pub acquired_at: Timestamp,
    pub value: Option<f64>,
    pub unit: String,
    pub availability: Availability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub id: String,
    pub vehicle_id: String,
    pub label: String,
    pub interval_seconds: u64,
    pub source: SourceMode,
    pub simulated: bool,
    pub generation: u64,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    pub reason: Option<String>,
    pub snapshot_count: u64,
}
impl SessionMetadata {
    pub fn new(vehicle_id: String, label: String, interval_seconds: u64, source: SourceMode, generation: u64, started_at: Timestamp) -> Self {
        let simulated = matches!(source, SourceMode::Demo | SourceMode::Replay);
        Self { id: uuid::Uuid::new_v4().to_string(), vehicle_id, label, interval_seconds, source, simulated, generation, started_at, ended_at: None, reason: None, snapshot_count: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleRow {
    pub row_id: i64,
    pub session_id: String,
    pub elapsed_ms: u64,
    pub captured_at: Timestamp,
    pub age_ms: u64,
    pub stale: bool,
    pub pre_start: bool,
    pub repeated: bool,
    pub measurement: Measurement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplePage { pub rows: Vec<SampleRow>, pub next_cursor: Option<i64>, pub truncated: bool }

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct RecordingError { pub code: &'static str, pub message: String }
impl RecordingError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self { Self { code, message: message.into() } }
}
impl From<sqlite::Error> for RecordingError {
    fn from(_: sqlite::Error) -> Self { Self::new("storage", "The local diagnostics database could not be read or written.") }
}
impl From<serde_json::Error> for RecordingError {
    fn from(_: serde_json::Error) -> Self { Self::new("storage", "Stored diagnostics data is invalid.") }
}
