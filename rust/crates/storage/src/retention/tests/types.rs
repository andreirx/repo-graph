//! Type tests for RetentionClass.

use crate::retention::RetentionClass;

#[test]
fn retention_class_roundtrip() {
    for class in [
        RetentionClass::Current,
        RetentionClass::Parent,
        RetentionClass::BaselineAuto,
        RetentionClass::BaselineUser,
        RetentionClass::Prunable,
    ] {
        let s = class.as_str();
        let parsed = RetentionClass::parse(s).unwrap();
        assert_eq!(class, parsed);
    }
}

#[test]
fn protection_status() {
    assert!(RetentionClass::Current.is_protected());
    assert!(RetentionClass::Parent.is_protected());
    assert!(RetentionClass::BaselineAuto.is_protected());
    assert!(RetentionClass::BaselineUser.is_protected());
    assert!(!RetentionClass::Prunable.is_protected());
}
