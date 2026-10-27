// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The only way this crate writes to disk.

use std::io::Write;
use std::path::Path;

use crate::Error;

// Writes a sibling temp file, flushes it to the platter, then renames over
// the target. A reader sees either the whole old file or the whole new
// one. std::fs::write leaves a truncated file behind if the process dies
// mid-write, and truncated settings.json reads back as "use defaults".
pub fn write(path: &Path, contents: &str) -> Result<(), Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|source| Error::Write {
        path: parent.to_path_buf(),
        source,
    })?;

    // Same directory, so the rename never crosses a filesystem. The
    // process id keeps two instances off one temp name.
    let temp_path = path.with_extension(format!("tmp{}", std::process::id()));

    let write_temp = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(contents.as_bytes())?;
        // Without this the rename can land before the bytes do, and a
        // crash in between leaves an intact-looking empty file.
        file.sync_all()?;
        Ok(())
    };

    if let Err(source) = write_temp() {
        let _ = std::fs::remove_file(&temp_path);
        return Err(Error::Write {
            path: temp_path,
            source,
        });
    }

    if let Err(source) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(Error::Write {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("weglet-atomic-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_write_lands_whole() {
        let path = dir("whole").join("f.json");
        write(&path, "a = 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 1\n");
    }

    // The failure this exists to prevent: a shorter write leaving the
    // tail of the previous contents behind.
    #[test]
    fn a_shorter_write_replaces_the_longer_one_completely() {
        let path = dir("replace").join("f.json");
        write(&path, &"a".repeat(4096)).unwrap();
        write(&path, "b").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "b");
    }

    #[test]
    fn missing_directories_are_created() {
        let path = dir("nested").join("a").join("b").join("f.json");
        write(&path, "x").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn no_temp_file_is_left_behind() {
        let dir = dir("cleanup");
        write(&dir.join("f.json"), "x").unwrap();
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("tmp"))
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }
}
