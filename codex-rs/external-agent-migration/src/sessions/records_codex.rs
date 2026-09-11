use codex_protocol::ThreadId;
use codex_rollout::RolloutItem;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;

pub(crate) fn validate_codex_rollout(path: &Path) -> io::Result<()> {
    let reader = BufReader::new(File::open(path)?);
    let mut records = 0_usize;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        serde_json::from_str::<RolloutItem>(&line)
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

pub(crate) fn codex_rollout_thread_id(path: &Path) -> io::Result<ThreadId> {
    let reader = BufReader::new(File::open(path)?);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let item = serde_json::from_str::<RolloutItem>(&line)
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
