use super::*;

#[test]
fn seconds_to_ms_basic() {
    assert_eq!(seconds_to_ms(0.0), Some(0), "zero seconds");
    assert_eq!(seconds_to_ms(1.0), Some(1000), "one second");
    assert_eq!(seconds_to_ms(0.001), Some(1), "one millisecond");
    assert_eq!(seconds_to_ms(0.5), Some(500), "half second");
}

#[test]
fn seconds_to_ms_rejects_bad_values() {
    assert_eq!(seconds_to_ms(-1.0), None, "negative");
    assert_eq!(seconds_to_ms(f64::NAN), None, "NaN");
    assert_eq!(seconds_to_ms(f64::INFINITY), None, "infinity");
    assert_eq!(seconds_to_ms(f64::NEG_INFINITY), None, "neg infinity");
}
