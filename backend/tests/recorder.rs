use obdium::recording::*;
use std::sync::Arc;

#[test]
fn manual_recording_ticks_on_monotonic_time_and_stops_on_disconnect() {
    let cache=Arc::new(MeasurementCache::default()); cache.reset(1);
    let store=RecordingStore::open(std::path::Path::new(":memory:")).unwrap();
    let mut recorder=Recorder::new(store,cache);
    assert!(recorder.start(1,"Test".into(),Timestamp{utc_ms:1000,elapsed_ms:0}).is_err());
    recorder.set_connection(Some(ConnectionContext {generation:1,vehicle_id:"vehicle1".into(),source:SourceMode::Live,simulated:false}));
    let session=recorder.start(1,"Test".into(),Timestamp{utc_ms:1000,elapsed_ms:0}).unwrap();
    assert!(recorder.start(1,"Duplicate".into(),Timestamp::now()).is_err());
    for i in 1..=3 { recorder.tick(Timestamp{utc_ms:1000-i as i64*10,elapsed_ms:i*1000}).unwrap(); }
    assert_eq!(recorder.status().snapshot_count,4);
    recorder.stop("disconnect",Timestamp{utc_ms:500,elapsed_ms:3500}).unwrap();
    recorder.set_connection(None);
    assert!(!recorder.status().recording);
    assert_eq!(recorder.store.session(&session.id).unwrap().reason.as_deref(),Some("disconnect"));
    recorder.set_connection(Some(ConnectionContext {generation:2,vehicle_id:"vehicle1".into(),source:SourceMode::Live,simulated:false}));
    assert!(!recorder.status().recording);
    assert!(recorder.start(3,"Invalid interval".into(),Timestamp::now()).is_err());
}

#[test]
fn reopen_marks_active_recordings_interrupted() {
    let mut store=RecordingStore::open(std::path::Path::new(":memory:")).unwrap();
    let session=SessionMetadata::new("v".into(),"Interrupted".into(),1,SourceMode::Demo,1,Timestamp::now());
    store.insert_session(&session).unwrap();
    let recorder=Recorder::new(store,Arc::new(MeasurementCache::default()));
    assert_eq!(recorder.store.session(&session.id).unwrap().reason.as_deref(),Some("interrupted"));
}
