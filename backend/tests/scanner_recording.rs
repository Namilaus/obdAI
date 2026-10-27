use obdium::{scalar::{Scalar,Unit}, OBD};

#[test]
fn acquisition_timestamp_is_preserved_until_display() {
    let value=Scalar::new(12.375,Unit::Volts,None);
    let timestamp=value.acquired_at.clone();
    let displayed=value.value.round();
    assert_eq!(displayed,12.0);
    assert_eq!(value.value,12.375);
    assert_eq!(value.acquired_at,timestamp);
}

#[test]
fn failed_dtc_scan_is_not_an_empty_success() {
    let mut obd=OBD::new();
    assert!(obd.scan_trouble_codes().is_err());
    assert!(obd.scan_permanent_trouble_codes().is_err());
}
