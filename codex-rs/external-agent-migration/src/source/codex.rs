use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn discover_rollouts(source_home: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in [
        source_home.join("sessions"),
        source_home.join("archived_sessions"),
    ] {
        if root.is_dir() {
            collect_jsonl(&root, &mut files)?;
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_jsonl(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_jsonl(&path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        {
            files.push(path);
        }
    }
    Ok(())
}
