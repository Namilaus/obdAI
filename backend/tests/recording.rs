use obdium::recording::{Availability, Measurement, MeasurementCache, RecordingStore, SessionMetadata, SourceMode, Timestamp};

fn measurement(value: Option<f64>, id: u64, generation: u64) -> Measurement {
    Measurement { metric_id: "engine.rpm".into(), name: "Engine RPM".into(), observation_id: id,
        generation, acquired_at: Timestamp { utc_ms: 1000, elapsed_ms: id * 100 },
        value, unit: "RPM".into(), availability: if value.is_some() { Availability::Valid } else { Availability::NoData } }
}

#[test]
fn cache_preserves_precision_zero_missing_and_generations() {
    let cache = MeasurementCache::default();
    cache.reset(1);
    cache.publish(measurement(Some(12.375), 1, 1)).unwrap();
    assert_eq!(cache.snapshot(1)[0].value, Some(12.375));
    cache.publish(measurement(Some(0.0), 2, 1)).unwrap();
    assert_eq!(cache.snapshot(1)[0].value, Some(0.0));
    cache.publish(measurement(None, 3, 1)).unwrap();
    assert_eq!(cache.snapshot(1)[0].value, None);
    cache.reset(2);
    assert!(cache.snapshot(2).is_empty());
    assert!(cache.publish(measurement(Some(1.0), 4, 1)).is_err());
}

#[test]
fn recording_store_reopens_precision_and_isolates_bundled_databases() {
    let path = std::env::temp_dir().join(format!("obdium-recording-{}.sqlite", std::process::id()));
    let session = SessionMetadata::new("test-vehicle".into(), "Test".into(), 1, SourceMode::Live, 1,
        Timestamp { utc_ms: 1000, elapsed_ms: 0 });
    {
        let mut store = RecordingStore::open(&path).unwrap();
        store.insert_session(&session).unwrap();
        store.append_snapshot(&session.id, Timestamp { utc_ms: 1200, elapsed_ms: 200 }, &[measurement(Some(12.375), 1, 1)]).unwrap();
        store.finalize(&session.id, "user", Timestamp { utc_ms: 1400, elapsed_ms: 400 }).unwrap();
    }
    {
        let store = RecordingStore::open(&path).unwrap();
        assert_eq!(store.list_sessions(100, 0).unwrap()[0].id, session.id);
        assert_eq!(store.samples(&session.id, &[], 0, u64::MAX, 500, 0).unwrap().rows[0].measurement.value, Some(12.375));
    }
    let _ = std::fs::remove_file(path);
}
