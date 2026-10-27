// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The error type for reads and writes in this crate.

use std::path::PathBuf;

// Carries the path in every variant: "failed to parse settings" with no
// file name is a bug report nobody can act on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not valid JSON")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json_lenient::Error,
    },

    // The TOML file an older Weglet wrote, which store.rs converts once.
    #[error("{path} is not valid TOML")]
    LegacyParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("could not serialise {path}")]
    Serialise {
        path: PathBuf,
        #[source]
        source: serde_json_lenient::Error,
    },

    #[error("no profile directory on this platform")]
    NoProfileDirectory,
}
