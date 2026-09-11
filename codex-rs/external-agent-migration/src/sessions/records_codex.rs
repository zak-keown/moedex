use codex_protocol::ThreadId;
use codex_rollout::RolloutItem;
use std::io;

pub(crate) fn validate_codex_rollout(bytes: &[u8]) -> io::Result<()> {
    let mut records = 0_usize;
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        serde_json::from_slice::<RolloutItem>(line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        records = records.saturating_add(1);
    }
    if records == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Codex rollout contains no records",
        ));
    }
    Ok(())
}

pub(crate) fn codex_rollout_thread_id(bytes: &[u8]) -> io::Result<ThreadId> {
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let item = serde_json::from_slice::<RolloutItem>(line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if let RolloutItem::SessionMeta(meta) = item {
            return Ok(meta.meta.id);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Codex rollout contains no session metadata",
    ))
}

#[cfg(test)]
#[path = "records_codex_tests.rs"]
mod tests;
