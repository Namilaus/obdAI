use obdium::{
    scalar::{Scalar, Unit},
    OBD,
};

#[test]
fn acquisition_timestamp_is_preserved_until_display() {
    let value = Scalar::new(12.375, Unit::Volts, None);
    let timestamp = value.acquired_at.clone();
    let displayed = value.value.round();
    assert_eq!(displayed, 12.0);
    assert_eq!(value.value, 12.375);
    assert_eq!(value.acquired_at, timestamp);
}

#[test]
fn failed_dtc_scan_is_not_an_empty_success() {
    let mut obd = OBD::new();
    assert!(obd.scan_trouble_codes().is_err());
    assert!(obd.scan_permanent_trouble_codes().is_err());
}
#[test]
fn dtc_low_zero_byte_is_not_removed_as_padding() {
    let mut response = "43 01 00 00 00".to_string();
    let codes = OBD::decode_trouble_codes(&mut response);
    assert_eq!(codes.len(), 1);
    assert_eq!(codes[0].dtc, "P0100");
}
#[test]
fn dtc_data_equal_to_service_byte_is_not_a_new_header() {
    let mut response = "43 01 43 00 00".to_string();
    let codes = OBD::decode_trouble_codes(&mut response);
    assert_eq!(codes.len(), 1);
    assert_eq!(codes[0].dtc, "P0143");
}
#[test]
fn can_dtc_count_and_ecu_boundaries_are_preserved() {
    let mut response = "7E8 04 43 01 01 43\r7E9 04 43 01 01 00\r>".to_string();
    let codes = OBD::decode_trouble_codes(&mut response);
    let mut names: Vec<_> = codes.iter().map(|c| c.dtc.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["P0100", "P0143"]);
}
#[test]
fn split_29bit_can_and_legacy_headers_are_decoded() {
    for raw in [
        "18 DA F1 10 04 43 01 01 33\r>",
        "48 6B 10 43 01 33 00 00 00 00 3A\r>",
    ] {
        let codes = OBD::decode_trouble_codes(&mut raw.to_string());
        assert_eq!(codes.len(), 1, "{raw}");
        assert_eq!(codes[0].dtc, "P0133");
    }
}
