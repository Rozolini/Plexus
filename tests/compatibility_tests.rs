use plexus::{ArenaHeader, ArenaVersion, CompatibilityResult};

#[test]
fn header_major_mismatch_is_detectable() {
    let mut header = ArenaHeader::new(1024, 1, 1);
    header.version = ArenaVersion { major: 1, minor: 0 };
    header.checksum = header.calculate_checksum();
    assert_eq!(
        header.compatibility_with(ArenaVersion::CURRENT, u64::MAX),
        CompatibilityResult::IncompatibleMajor
    );
}

#[test]
fn header_minor_forward_compatibility_flagged() {
    let mut header = ArenaHeader::new(1024, 1, 1);
    header.version = ArenaVersion {
        major: ArenaVersion::CURRENT.major,
        minor: ArenaVersion::CURRENT.minor + 1,
    };
    header.checksum = header.calculate_checksum();
    assert_eq!(
        header.compatibility_with(ArenaVersion::CURRENT, u64::MAX),
        CompatibilityResult::ForwardCompatibleReadOnly
    );
}

#[test]
fn header_feature_flag_incompatibility_is_detectable() {
    let mut header = ArenaHeader::new(1024, 1, 1);
    header.feature_flags = 0b0100;
    header.checksum = header.calculate_checksum();
    assert_eq!(
        header.compatibility_with(ArenaVersion::CURRENT, 0b0011),
        CompatibilityResult::IncompatibleFeatureFlags
    );
}

#[test]
fn header_compatibility_accepts_supported_flags_and_minor() {
    let mut header = ArenaHeader::new(1024, 1, 1);
    header.feature_flags = 0b0011;
    header.version = ArenaVersion::CURRENT;
    header.checksum = header.calculate_checksum();
    assert_eq!(
        header.compatibility_with(ArenaVersion::CURRENT, 0b0011),
        CompatibilityResult::Compatible
    );
}
