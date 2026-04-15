#![cfg(windows)]

use plexus::{MapAccess, MappingSecurityProfile, SharedEndpoint};

#[test]
fn endpoint_can_bootstrap_from_mapping_without_user_unsafe() {
    let name = format!("Local\\plexus-safe-endpoint-{}", std::process::id());
    let endpoint =
        SharedEndpoint::<u64>::open_or_create_mapping(&name, 64 * 1024, MapAccess::ReadWrite, 128)
            .expect("endpoint");

    endpoint.send_copy(7).expect("send");
    let value = endpoint.try_recv_copy().expect("recv");
    assert_eq!(value, 7);
}

#[test]
fn endpoint_supports_acl_profile_safe_bootstrap() {
    let name = format!("Local\\plexus-safe-endpoint-acl-{}", std::process::id());
    let endpoint = SharedEndpoint::<u64>::open_or_create_mapping_with_profile(
        &name,
        64 * 1024,
        MapAccess::ReadWrite,
        MappingSecurityProfile::ProducerConsumerLocalUsers,
        128,
    )
    .expect("endpoint");

    endpoint.send_copy(9).expect("send");
    let value = endpoint.try_recv_copy().expect("recv");
    assert_eq!(value, 9);
}
