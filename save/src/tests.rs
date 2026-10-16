use super::*;

fn decode(bytes: &[u8]) -> Option<u32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}
fn encode(value: &u32) -> io::Result<Vec<u8>> {
    Ok(value.to_string().into_bytes())
}

/// A unique directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "game_save-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn missing_and_invalid_documents_are_repaired_on_disk() {
    let directory = TempDir::new();
    let path = directory.path().join("nested/progress");
    let file = SaveFile::new(&path);
    assert_eq!(file.load(decode, encode).status, LoadStatus::Created);
    assert_eq!(fs::read(&path).unwrap(), b"0");
    for bad in [b"invalid".as_slice(), b"\xff\xfe".as_slice()] {
        fs::write(&path, bad).unwrap();
        let loaded = file.load(decode, encode);
        assert_eq!(loaded.status, LoadStatus::ResetInvalid);
        assert_eq!(loaded.value, 0);
        assert_eq!(fs::read(&path).unwrap(), b"0");
        assert!(
            !rejected_path(&path).exists(),
            "rejected saves are kept only on request"
        );
    }
    file.write(b"42").unwrap();
    let loaded = file.load(decode, encode);
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert_eq!(loaded.value, 42);
}

#[test]
fn opted_in_rejected_documents_are_set_aside_before_repair() {
    let directory = TempDir::new();
    let path = directory.path().join("nested/progress");
    let file = SaveFile::new(&path).keep_rejected();
    assert_eq!(file.load(decode, encode).status, LoadStatus::Created);
    assert_eq!(fs::read(&path).unwrap(), b"0");
    assert!(
        !rejected_path(&path).exists(),
        "a missing save is not rejected"
    );
    for bad in [b"invalid".as_slice(), b"\xff\xfe".as_slice()] {
        fs::write(&path, bad).unwrap();
        let loaded = file.load(decode, encode);
        assert_eq!(loaded.status, LoadStatus::ResetInvalid);
        assert_eq!(loaded.value, 0);
        assert_eq!(fs::read(&path).unwrap(), b"0");
        assert_eq!(
            fs::read(rejected_path(&path)).unwrap(),
            bad,
            "the rejected save was destroyed"
        );
    }
    file.write(b"42").unwrap();
    let loaded = file.load(decode, encode);
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert_eq!(loaded.value, 42);
    assert_eq!(fs::read(rejected_path(&path)).unwrap(), b"\xff\xfe");
}

#[test]
fn rejected_path_appends_to_the_full_file_name() {
    assert_eq!(
        rejected_path(Path::new("saves/progress.json")),
        PathBuf::from("saves/progress.json.invalid")
    );
}

#[test]
fn writes_store_exactly_the_given_bytes_without_staging_leftovers() {
    let directory = TempDir::new();
    let path = directory.path().join("progress");
    let file = SaveFile::new(&path);
    let bytes = b"{\"v\":1}\n\0\xff";
    file.write(bytes).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(!directory.path().join("progress.tmp").exists());
}

#[test]
fn memory_files_load_defaults_and_accept_writes() {
    let file = SaveFile::memory().keep_rejected();
    assert_eq!(file.path(), None);
    let loaded = file.load(decode, encode);
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert_eq!(loaded.value, 0);
    file.write(b"42").unwrap();
}

#[test]
fn failed_staging_preserves_the_previous_complete_file() {
    let directory = TempDir::new();
    let path = directory.path().join("progress");
    let file = SaveFile::new(&path);
    file.write(b"42").unwrap();
    fs::create_dir(directory.path().join("progress.tmp")).unwrap();
    assert!(file.write(b"99").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"42");
}

#[test]
fn read_failure_blocks_automatic_saves_across_clones() {
    let directory = TempDir::new();
    let path = directory.path().join("progress");
    fs::create_dir(&path).unwrap();
    let file = SaveFile::new(&path);
    let clone = file.clone();
    assert!(matches!(
        file.load(decode, encode).status,
        LoadStatus::ReadUnavailable(_)
    ));
    fs::remove_dir(&path).unwrap();
    fs::write(&path, b"42").unwrap();
    // The initial I/O failure is over, but default-state autosaves are
    // still forbidden until an explicit load recovers the authoritative data.
    assert!(clone.write(b"0").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"42");
    assert_eq!(clone.load(decode, encode).value, 42);
    file.write(b"43").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"43");
}

#[cfg(unix)]
#[test]
fn unreadable_file_in_writable_directory_is_preserved() {
    use std::os::unix::fs::PermissionsExt;
    let directory = TempDir::new();
    let path = directory.path().join("progress");
    fs::write(&path, b"42").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&path).is_ok() {
        // Root bypasses POSIX mode bits; the portable directory/recovery
        // case above still exercises the sticky write guard there.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        return;
    }
    let file = SaveFile::new(&path).keep_rejected();
    assert_eq!(
        file.load(decode, encode).status,
        LoadStatus::ReadUnavailable(io::ErrorKind::PermissionDenied)
    );
    assert!(file.write(b"0").is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(file.write(b"0").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"42");
    assert!(
        !rejected_path(&path).exists(),
        "an unreadable save is not rejected"
    );
}
