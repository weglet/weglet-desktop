// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// How a profile store reaches the disk: JSON, written whole through
// atomic::write. Read leniently, so a file edited by hand may carry
// comments and trailing commas -- though a store's own file is rewritten
// whole on the next save, and its comments with it.
//
// An older Weglet wrote these files as TOML. A store with no JSON file yet
// but a TOML one beside it is converted once: the JSON is written, and the
// TOML is removed only after that write succeeded.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::Error;

// The store's contents, its defaults when neither file exists.
pub(crate) fn load<T: DeserializeOwned + Default>(path: &Path) -> Result<T, Error> {
    migrate_legacy_toml(path)?;
    match read(path)? {
        Some(text) => parse(path, &text),
        None => Ok(T::default()),
    }
}

// As load, but never writes: for a profile this process does not run,
// whose files belong to whichever process opens it. A TOML file is read
// where it is, not converted.
pub(crate) fn peek<T: DeserializeOwned + Default>(path: &Path) -> Result<T, Error> {
    if let Some(text) = read(path)? {
        return parse(path, &text);
    }
    let legacy = legacy_toml_path(path);
    match read(&legacy)? {
        Some(text) => toml::from_str(&text).map_err(|source| Error::LegacyParse {
            path: legacy,
            source,
        }),
        None => Ok(T::default()),
    }
}

fn parse<T: DeserializeOwned>(path: &Path, text: &str) -> Result<T, Error> {
    serde_json_lenient::from_str(text).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), Error> {
    let text = serde_json_lenient::to_string_pretty(value).map_err(|source| Error::Serialise {
        path: path.to_path_buf(),
        source,
    })?;
    crate::atomic::write(path, &text)
}

// Converts the TOML file an older Weglet kept for `json_path`, if there is
// one and no JSON file yet. Also for files a user writes by hand, such as a
// profile's own brands or engines table: those are converted the same way.
pub fn migrate_legacy_toml(json_path: &Path) -> Result<(), Error> {
    let legacy = legacy_toml_path(json_path);
    if json_path.exists() {
        return Ok(());
    }
    let Some(text) = read(&legacy)? else {
        return Ok(());
    };
    let value: toml::Value = toml::from_str(&text).map_err(|source| Error::LegacyParse {
        path: legacy.clone(),
        source,
    })?;
    save(json_path, &value)?;
    // NotFound: another process converted the same file at the same time.
    // Its JSON is the same as this one, and the TOML is gone either way.
    match std::fs::remove_file(&legacy) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Write {
            path: legacy,
            source,
        }),
    }
}

fn legacy_toml_path(json_path: &Path) -> PathBuf {
    json_path.with_extension("toml")
}

fn read(path: &Path) -> Result<Option<String>, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Deserialize, Serialize)]
    #[serde(default, rename_all = "kebab-case")]
    struct Sample {
        name: String,
        count: u64,
        tags: Vec<String>,
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("weglet-store-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_store_loads_as_its_defaults() {
        let path = dir("missing").join("sample.json");
        assert_eq!(load::<Sample>(&path).unwrap(), Sample::default());
    }

    #[test]
    fn a_store_round_trips_as_json() {
        let path = dir("roundtrip").join("sample.json");
        let sample = Sample {
            name: "a".to_string(),
            count: 2,
            tags: vec!["x".to_string()],
        };
        save(&path, &sample).unwrap();
        assert_eq!(load::<Sample>(&path).unwrap(), sample);
    }

    #[test]
    fn a_hand_edited_file_may_carry_comments_and_trailing_commas() {
        let path = dir("lenient").join("sample.json");
        std::fs::write(&path, "{\n  // mine\n  \"name\": \"a\",\n  \"tags\": [\"x\",],\n}\n").unwrap();
        let sample: Sample = load(&path).unwrap();
        assert_eq!(sample.name, "a");
        assert_eq!(sample.tags, ["x"]);
    }

    #[test]
    fn a_legacy_toml_store_is_converted_and_removed() {
        let dir = dir("legacy");
        std::fs::write(dir.join("sample.toml"), "name = \"a\"\ncount = 2\ntags = [\"x\"]\n").unwrap();
        let sample: Sample = load(&dir.join("sample.json")).unwrap();
        assert_eq!(
            sample,
            Sample {
                name: "a".to_string(),
                count: 2,
                tags: vec!["x".to_string()],
            }
        );
        assert!(dir.join("sample.json").exists());
        assert!(!dir.join("sample.toml").exists());
    }

    // Converting twice would replace newer JSON with the old TOML.
    #[test]
    fn an_existing_json_store_wins_over_a_leftover_toml_one() {
        let dir = dir("both");
        std::fs::write(dir.join("sample.toml"), "name = \"old\"\n").unwrap();
        std::fs::write(dir.join("sample.json"), "{\"name\": \"new\"}").unwrap();
        let sample: Sample = load(&dir.join("sample.json")).unwrap();
        assert_eq!(sample.name, "new");
    }

    #[test]
    fn peeking_reads_a_legacy_file_without_converting_it() {
        let dir = dir("peek");
        std::fs::write(dir.join("sample.toml"), "name = \"a\"\n").unwrap();
        let sample: Sample = peek(&dir.join("sample.json")).unwrap();
        assert_eq!(sample.name, "a");
        assert!(dir.join("sample.toml").exists());
        assert!(!dir.join("sample.json").exists());
    }

    // The TOML is the only copy until the JSON is written: it must survive
    // a conversion that fails.
    #[test]
    fn a_legacy_file_that_will_not_parse_is_kept() {
        let dir = dir("legacy-broken");
        std::fs::write(dir.join("sample.toml"), "name = = =\n").unwrap();
        assert!(matches!(
            load::<Sample>(&dir.join("sample.json")),
            Err(Error::LegacyParse { .. })
        ));
        assert!(dir.join("sample.toml").exists());
        assert!(!dir.join("sample.json").exists());
    }
}
