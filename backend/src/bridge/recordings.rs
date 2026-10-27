use obdium::recording::*;

pub async fn perform<T: Send + 'static>(
    work: impl FnOnce(&mut Recorder) -> Result<T, RecordingError> + Send + 'static,
) -> Result<T, RecordingError> {
    let handle = ACTIVE_RECORDER.get().cloned().ok_or_else(|| {
        RecordingError::new(
            "storage",
            "Local recordings are unavailable. Check the application data directory and restart.",
        )
    })?;
    tauri::async_runtime::spawn_blocking(move || handle.call(work))
        .await
        .map_err(|_| RecordingError::new("storage", "Recording request was interrupted."))?
}
pub async fn read<T: Send + 'static>(
    work: impl FnOnce(&RecordingStore) -> Result<T, RecordingError> + Send + 'static,
) -> Result<T, RecordingError> {
    let path = ACTIVE_RECORDER
        .get()
        .ok_or_else(|| RecordingError::new("storage", "Recording storage is unavailable."))?
        .path
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = RecordingStore::open_reader(&path)?;
        work(&store)
    })
    .await
    .map_err(|_| RecordingError::new("storage", "History query was interrupted."))?
}
#[tauri::command]
pub async fn recording_start(
    interval_seconds: u64,
    label: String,
) -> Result<SessionMetadata, RecordingError> {
    perform(move |r| r.start(interval_seconds, label, Timestamp::now())).await
}
#[tauri::command]
pub async fn recording_stop() -> Result<SessionMetadata, RecordingError> {
    perform(|r| r.stop("user", Timestamp::now())).await
}
#[tauri::command]
pub async fn recording_status() -> Result<RecordingStatus, RecordingError> {
    perform(|r| Ok(r.status())).await
}
#[tauri::command]
pub async fn recording_list(
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<SessionMetadata>, RecordingError> {
    read(move |store| store.list_sessions(limit.unwrap_or(50), offset.unwrap_or(0))).await
}
#[tauri::command]
pub async fn recording_metrics(id: String) -> Result<Vec<Measurement>, RecordingError> {
    read(move |store| store.metrics(&id)).await
}
#[tauri::command]
pub async fn recording_samples(
    id: String,
    metrics: Option<Vec<String>>,
    start: Option<u64>,
    end: Option<u64>,
    limit: Option<usize>,
    cursor: Option<i64>,
) -> Result<SamplePage, RecordingError> {
    read(move |store| {
        store.samples(
            &id,
            &metrics.unwrap_or_default(),
            start.unwrap_or(0),
            end.unwrap_or(u64::MAX),
            limit.unwrap_or(100),
            cursor.unwrap_or(0),
        )
    })
    .await
}
#[tauri::command]
pub async fn recording_summary(
    id: String,
    metrics: Option<Vec<String>>,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<Vec<MetricSummary>, RecordingError> {
    read(move |store| {
        store.summary(
            &id,
            &metrics.unwrap_or_default(),
            start.unwrap_or(0),
            end.unwrap_or(u64::MAX),
        )
    })
    .await
}
#[tauri::command]
pub async fn recording_assign_vehicle(
    id: String,
    vehicle_id: String,
    label: String,
) -> Result<SessionMetadata, RecordingError> {
    perform(move |r| r.store.assign_vehicle(&id, &vehicle_id, &label)).await
}
#[tauri::command]
pub async fn recording_delete_preview(id: String) -> Result<DeletionPreview, RecordingError> {
    read(move |store| store.delete_preview(&id)).await
}
#[tauri::command]
pub async fn recording_delete(id: String, revision_token: String) -> Result<(), RecordingError> {
    if let Some(service) = obdium::ai::AI_SERVICE.get() {
        let preview = read({
            let id = id.clone();
            move |store| store.delete_preview(&id)
        })
        .await?;
        for conversation in preview.conversation_ids {
            service.cancel_conversation(&conversation);
        }
    }
    perform(move |r| r.store.delete_recording(&id, &revision_token)).await
}
