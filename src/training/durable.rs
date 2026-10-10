//! Publish complete files without truncating the last successful save.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn write(path: &Path, bytes: &[u8], backup: bool) -> Result<(), String> {
    publish(path, backup, |f| f.write_all(bytes))
        .map_err(|e| format!("atomic write {}: {e}", path.display()))
}

fn publish(
    path: &Path,
    backup: bool,
    writer: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("missing filename"))?
        .to_string_lossy();
    let nonce = format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let temporary = parent.join(format!(".{name}.{nonce}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        writer(&mut file)?;
        file.sync_all()?;
        if backup && path.exists() {
            let previous = parent.join(format!("{name}.previous"));
            let staged = parent.join(format!(".{name}.previous.{nonce}.tmp"));
            // A hard link preserves the old inode without copying checkpoint bytes.
            fs::hard_link(path, &staged)?;
            if let Err(e) = fs::rename(&staged, &previous) {
                let _ = fs::remove_file(staged);
                return Err(e);
            }
            File::open(parent)?.sync_all()?;
        }
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_write_preserves_current_and_previous_checkpoint() {
        let dir = std::env::temp_dir().join(format!("durable-save-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        write(&path, b"old", true).unwrap();
        write(&path, b"current", true).unwrap();
        let error = publish(&path, true, |f| {
            f.write_all(b"partial")?;
            Err(io::Error::from_raw_os_error(libc::ENOSPC))
        })
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
        assert_eq!(fs::read(&path).unwrap(), b"current");
        assert_eq!(fs::read(dir.join("state.json.previous")).unwrap(), b"old");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        write(&path, b"new", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(
            fs::read(dir.join("state.json.previous")).unwrap(),
            b"current"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
