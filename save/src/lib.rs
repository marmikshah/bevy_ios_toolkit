//! Safe current-format saves, without Bevy or platform SDK dependencies.
//!
//! A failed read disables writes for this file and every clone until a later
//! explicit load succeeds. Returning defaults cannot authorize an autosave over
//! unreadable progress. Contents the codec rejects are reset.
//! [`SaveFile::keep_rejected`] opts in to setting the rejected file aside
//! first (at [`rejected_path`]), so a codec or content change that rejects
//! real progress never destroys it.
//!
//! The crate moves opaque bytes: the game's codec owns the format. Writes are
//! atomic: bytes go to `<save>.tmp`, are synced, then renamed over the save.
//!
//! ```toml
//! [dependencies]
//! game_save = { git = "https://github.com/marmikshah/bevy_ios_toolkit", rev = "…" }
//! ```
//!
//! ```no_run
//! use game_save::{LoadStatus, SaveFile};
//!
//! let file = SaveFile::new("progress.txt").keep_rejected();
//! let loaded = file.load(
//!     |bytes| std::str::from_utf8(bytes).ok()?.parse::<u32>().ok(),
//!     |best| Ok(best.to_string().into_bytes()),
//! );
//! if loaded.status == LoadStatus::Loaded {
//!     file.write(b"42").unwrap();
//! }
//! ```

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStatus {
    Loaded,
    Created,
    ResetInvalid,
    ReadUnavailable(io::ErrorKind),
    RecoveryWriteFailed(io::ErrorKind),
}

pub struct Loaded<T> {
    pub value: T,
    pub status: LoadStatus,
}

#[derive(Debug, Clone)]
pub struct SaveFile {
    path: Option<PathBuf>,
    keep_rejected: bool,
    // Serialize clones against each other, including read/recovery and writes.
    access: Arc<Mutex<Option<io::ErrorKind>>>,
}

impl SaveFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            keep_rejected: false,
            access: Arc::new(Mutex::new(None)),
        }
    }

    pub fn memory() -> Self {
        Self {
            path: None,
            keep_rejected: false,
            access: Arc::new(Mutex::new(None)),
        }
    }

    /// Before a load resets a file its codec rejects, copy it to
    /// [`rejected_path`]. Off by default. The copy is best effort: the fresh
    /// save matters more than the keepsake.
    pub fn keep_rejected(mut self) -> Self {
        self.keep_rejected = true;
        self
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Only this explicit load can reopen writes after a read failure. The
    /// codec owns schema and semantic validation; no legacy fallback is used.
    pub fn load<T: Default>(
        &self,
        decode: impl Fn(&[u8]) -> Option<T>,
        encode: impl Fn(&T) -> io::Result<Vec<u8>>,
    ) -> Loaded<T> {
        let mut access = self
            .access
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(path) = &self.path else {
            return Loaded {
                value: T::default(),
                status: LoadStatus::Loaded,
            };
        };
        let status = match fs::read(path) {
            Ok(bytes) => {
                *access = None;
                if let Some(value) = decode(&bytes) {
                    return Loaded {
                        value,
                        status: LoadStatus::Loaded,
                    };
                }
                LoadStatus::ResetInvalid
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                *access = None;
                LoadStatus::Created
            }
            Err(error) => {
                *access = Some(error.kind());
                return Loaded {
                    value: T::default(),
                    status: LoadStatus::ReadUnavailable(error.kind()),
                };
            }
        };
        if self.keep_rejected && status == LoadStatus::ResetInvalid {
            let _ = fs::copy(path, rejected_path(path));
        }
        let value = T::default();
        let result = encode(&value).and_then(|bytes| write_atomic(path, &bytes));
        Loaded {
            value,
            status: result.map_or_else(
                |error| LoadStatus::RecoveryWriteFailed(error.kind()),
                |_| status,
            ),
        }
    }

    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let access = self
            .access
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(kind) = *access {
            return Err(io::Error::new(
                kind,
                "save writes disabled after an unreadable file; load must recover first",
            ));
        }
        match &self.path {
            Some(path) => write_atomic(path, bytes),
            None => Ok(()),
        }
    }
}

/// Where [`SaveFile::keep_rejected`] keeps the last rejected save
/// (`<save>.invalid`). One copy: a later rejection replaces it.
pub fn rejected_path(path: &Path) -> PathBuf {
    let mut rejected = path.as_os_str().to_os_string();
    rejected.push(".invalid");
    PathBuf::from(rejected)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().filter(|path| !path.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent)?;
    }
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        // Best effort: some filesystems do not support directory fsync.
        #[cfg(not(target_os = "windows"))]
        if let Some(parent) = parent {
            let _ = fs::File::open(parent).and_then(|file| file.sync_all());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests;
