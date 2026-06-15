use std::fs;
use std::path::PathBuf;

use crate::{Error, entry::LifecycleEntry};

const FILE_PREFIX: &str = "lifecycle-run-";
const FILE_SUFFIX: &str = ".json";

#[derive(Debug, Clone)]
pub struct LifecycleLogger {
    dir: PathBuf,
}

impl LifecycleLogger {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn write(&self, entry: &LifecycleEntry) -> Result<PathBuf, Error> {
        fs::create_dir_all(&self.dir)?;
        let index = self.next_index()?;
        let path = self
            .dir
            .join(format!("{FILE_PREFIX}{index:02}{FILE_SUFFIX}"));
        fs::write(&path, serde_json::to_string_pretty(entry)?)?;
        Ok(path)
    }

    fn next_index(&self) -> Result<u32, Error> {
        let mut max = 0;
        if self.dir.exists() {
            for dir_entry in fs::read_dir(&self.dir)? {
                let name = dir_entry?.file_name();
                if let Some(index) = parse_index(&name.to_string_lossy()) {
                    max = max.max(index);
                }
            }
        }
        Ok(max + 1)
    }
}

fn parse_index(name: &str) -> Option<u32> {
    name.strip_prefix(FILE_PREFIX)?
        .strip_suffix(FILE_SUFFIX)?
        .parse()
        .ok()
}
