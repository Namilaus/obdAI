use obdium::recording::*;
fn fixture() -> (RecordingStore,SessionMetadata) {
    let mut store=RecordingStore::open(std::path::Path::new(":memory:")).unwrap();
    let session=SessionMetadata::new("v".into(),"test".into(),1,SourceMode::Live,1,Timestamp{utc_ms:0,elapsed_ms:0});
    store.insert_session(&session).unwrap(); (store,session)
}
fn value(v:f64,id:u64,at:u64,unit:&str)->Measurement {
    Measurement{metric_id:"temperature".into(),name:"Temperature".into(),observation_id:id,generation:1,acquired_at:Timestamp{utc_ms:at as i64,elapsed_ms:at},value:Some(v),unit:unit.into(),availability:Availability::Valid}
}
#[test]
fn summary_counts_observations_not_snapshot_duplicates() {
    let(mut store,s)=fixture();
    for (at,v) in [(0,value(10.0,1,0,"°C")),(1000,value(10.0,1,0,"°C")),(2000,value(20.0,2,2000,"°C")),(33000,value(20.0,2,2000,"°C"))] {
        store.append_snapshot(&s.id,Timestamp{utc_ms:at as i64,elapsed_ms:at},&[v]).unwrap();
    }
    let summary=store.summary(&s.id,&[],0,u64::MAX).unwrap();
    assert_eq!(summary[0].unique_observations,2);
    assert_eq!(summary[0].mean,Some(15.0));
    assert_eq!(summary[0].stale_snapshots,1);
    assert_eq!(summary[0].repeated_snapshots,2);
}
#[test]
fn summary_normalizes_units_without_rewriting_source() {
    let(mut store,s)=fixture();
    store.append_snapshot(&s.id,Timestamp{utc_ms:0,elapsed_ms:0},&[value(100.0,1,0,"°C")]).unwrap();
    store.append_snapshot(&s.id,Timestamp{utc_ms:1000,elapsed_ms:1000},&[value(212.0,2,1000,"°F")]).unwrap();
    assert_eq!(store.summary(&s.id,&[],0,u64::MAX).unwrap()[0].mean,Some(100.0));
    assert_eq!(store.samples(&s.id,&[],0,u64::MAX,500,0).unwrap().rows[1].measurement.unit,"°F");
    assert!(store.samples(&s.id,&[],0,10,501,0).is_err());
    assert!(store.samples(&s.id,&vec!["a".into();11],0,10,500,0).is_err());
}
