//! One-time migration from the app's former name, "Box": its data lived in
//! `%APPDATA%\com.box.app` (database `box.db`) and its snippets in
//! `Documents\Box Snippets`. Everything is moved, never copied, and only
//! when the new location does not exist yet.

use std::fs;
use std::path::{Path, PathBuf};

const OLD_IDENTIFIER: &str = "com.box.app";
const OLD_SNIPPETS_DIR: &str = "Box Snippets";

/// Moves the old app data directory to `data_dir` and renames the database.
pub fn data_dir(data_dir: &Path) {
    if let Some(parent) = data_dir.parent() {
        let old = parent.join(OLD_IDENTIFIER);
        if old.is_dir() && !data_dir.exists() {
            if let Err(e) = fs::rename(&old, data_dir) {
                eprintln!("cannot move {} to {}: {e}", old.display(), data_dir.display());
                return;
            }
        }
    }
    // The WAL and shared-memory files belong to the database by name.
    for suffix in ["", "-wal", "-shm"] {
        let old = data_dir.join(format!("box.db{suffix}"));
        let new = data_dir.join(format!("stash.db{suffix}"));
        if old.exists() && !new.exists() {
            let _ = fs::rename(old, new);
        }
    }
}

/// Renames the old default snippets directory to `new_dir`. Returns the
/// directory to use: `new_dir`, or the old one if it could not be renamed
/// (e.g. it is open in Explorer or held by a sync client).
pub fn snippets_dir(new_dir: &Path) -> PathBuf {
    let Some(old) = new_dir.parent().map(|p| p.join(OLD_SNIPPETS_DIR)) else {
        return new_dir.to_path_buf();
    };
    if !old.is_dir() || new_dir.exists() {
        return new_dir.to_path_buf();
    }
    match fs::rename(&old, new_dir) {
        Ok(()) => new_dir.to_path_buf(),
        Err(e) => {
            eprintln!("cannot rename {}: {e}; keeping it", old.display());
            old
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("stash-migrate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn moves_data_dir_and_database() {
        let root = temp_root("data");
        let old = root.join(OLD_IDENTIFIER);
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("box.db"), "db").unwrap();
        fs::write(old.join("box.db-wal"), "wal").unwrap();
        fs::write(old.join("config.json"), "{}").unwrap();

        let new = root.join("io.github.liuchang1437.stash");
        data_dir(&new);
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new.join("stash.db")).unwrap(), "db");
        assert_eq!(fs::read_to_string(new.join("stash.db-wal")).unwrap(), "wal");
        assert!(new.join("config.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn leaves_existing_new_dirs_alone() {
        let root = temp_root("existing");
        fs::create_dir_all(root.join(OLD_IDENTIFIER)).unwrap();
        fs::create_dir_all(root.join(OLD_SNIPPETS_DIR)).unwrap();
        let new_data = root.join("new");
        let new_snippets = root.join("Stash Snippets");
        fs::create_dir_all(&new_data).unwrap();
        fs::create_dir_all(&new_snippets).unwrap();

        data_dir(&new_data);
        assert_eq!(snippets_dir(&new_snippets), new_snippets);
        assert!(root.join(OLD_IDENTIFIER).exists());
        assert!(root.join(OLD_SNIPPETS_DIR).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn renames_snippets_dir() {
        let root = temp_root("snippets");
        fs::create_dir_all(root.join(OLD_SNIPPETS_DIR)).unwrap();
        fs::write(root.join(OLD_SNIPPETS_DIR).join("a.md"), "x").unwrap();
        let new = root.join("Stash Snippets");
        assert_eq!(snippets_dir(&new), new);
        assert!(new.join("a.md").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
