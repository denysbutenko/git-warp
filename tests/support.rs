use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

pub struct CurrentDirGuard {
    _lock: MutexGuard<'static, ()>,
    original: PathBuf,
}

impl CurrentDirGuard {
    pub fn new() -> Self {
        let lock = CURRENT_DIR_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original =
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        std::env::set_current_dir(&original).unwrap();

        Self {
            _lock: lock,
            original,
        }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
    }
}
