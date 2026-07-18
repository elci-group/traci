use crate::analyzer::{analyze_paths, collect_rust_files, Analysis};
use crate::config::Config;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime};

pub fn watch<F>(
    paths: &[PathBuf],
    config: &Config,
    interval: Duration,
    once: bool,
    mut on_analysis: F,
) -> Result<(), String>
where
    F: FnMut(&Analysis),
{
    let initial = analyze_paths(paths, config)?;
    on_analysis(&initial);
    if once {
        return Ok(());
    }

    let mut known = snapshot(paths, config)?;
    loop {
        thread::sleep(interval);
        let current = snapshot(paths, config)?;
        if current != known {
            known = current;
            on_analysis(&analyze_paths(paths, config)?);
        }
    }
}

fn snapshot(
    paths: &[PathBuf],
    config: &Config,
) -> Result<BTreeMap<PathBuf, (SystemTime, u64)>, String> {
    collect_rust_files(paths, config)?
        .into_iter()
        .map(|path| {
            let metadata = fs::metadata(&path)
                .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Ok((path, (modified, metadata.len())))
        })
        .collect()
}
