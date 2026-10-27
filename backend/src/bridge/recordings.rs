use obdium::recording::*;
use serde::Serialize;

pub async fn perform<T:Send+'static>(work:impl FnOnce(&mut Recorder)->Result<T,RecordingError>+Send+'static)->Result<T,RecordingError> {
    let handle=ACTIVE_RECORDER.get().cloned().ok_or_else(||RecordingError::new("storage","Local recordings are unavailable. Check the application data directory and restart."))?;
    tauri::async_runtime::spawn_blocking(move||handle.call(work)).await.map_err(|_|RecordingError::new("storage","Recording request was interrupted."))?
}
#[tauri::command]
pub async fn recording_start(interval_seconds:u64,label:String)->Result<SessionMetadata,RecordingError> { perform(move|r|r.start(interval_seconds,label,Timestamp::now())).await }
#[tauri::command]
pub async fn recording_stop()->Result<SessionMetadata,RecordingError> {perform(|r|r.stop("user",Timestamp::now())).await}
#[tauri::command]
pub async fn recording_status()->Result<RecordingStatus,RecordingError> {perform(|r|Ok(r.status())).await}
#[tauri::command]
pub async fn recording_list(limit:Option<usize>,offset:Option<usize>)->Result<Vec<SessionMetadata>,RecordingError> {perform(move|r|r.store.list_sessions(limit.unwrap_or(50),offset.unwrap_or(0))).await}
#[tauri::command]
pub async fn recording_metrics(id:String)->Result<Vec<Measurement>,RecordingError> {perform(move|r|r.store.metrics(&id)).await}
#[tauri::command]
pub async fn recording_samples(id:String,metrics:Option<Vec<String>>,start:Option<u64>,end:Option<u64>,limit:Option<usize>,cursor:Option<i64>)->Result<SamplePage,RecordingError> {
    perform(move|r|r.store.samples(&id,&metrics.unwrap_or_default(),start.unwrap_or(0),end.unwrap_or(u64::MAX),limit.unwrap_or(100),cursor.unwrap_or(0))).await
}
#[tauri::command]
pub async fn recording_summary(id:String,metrics:Option<Vec<String>>,start:Option<u64>,end:Option<u64>)->Result<Vec<MetricSummary>,RecordingError> {
    perform(move|r|r.store.summary(&id,&metrics.unwrap_or_default(),start.unwrap_or(0),end.unwrap_or(u64::MAX))).await
}
#[tauri::command]
pub async fn recording_assign_vehicle(id:String,vehicle_id:String,label:String)->Result<SessionMetadata,RecordingError> {perform(move|r|r.store.assign_vehicle(&id,&vehicle_id,&label)).await}
#[tauri::command]
pub async fn recording_delete_preview(id:String)->Result<DeletionPreview,RecordingError> {perform(move|r|r.store.delete_preview(&id)).await}
#[tauri::command]
pub async fn recording_delete(id:String,revision_token:String)->Result<(),RecordingError> {perform(move|r|r.store.delete_recording(&id,&revision_token)).await}

#[allow(dead_code)]
fn assert_serializable<T:Serialize>(_:T) {}
