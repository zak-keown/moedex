use super::validate_codex_rollout;

#[test]
fn rejects_an_empty_native_rollout() {
    let error = validate_codex_rollout(b"").expect_err("empty rollout must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}
