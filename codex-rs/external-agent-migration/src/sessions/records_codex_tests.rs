use super::validate_codex_rollout;
use tempfile::NamedTempFile;

#[test]
fn rejects_an_empty_native_rollout() {
    let rollout = NamedTempFile::new().expect("rollout");
    let error = validate_codex_rollout(rollout.path()).expect_err("empty rollout must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}
