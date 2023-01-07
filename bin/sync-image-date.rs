use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use filetime::FileTime;

fn find_video(path: &Path) -> Result<PathBuf> {
    // find f without extension
    let mut f = path.to_path_buf();
    f.set_extension("");
    if f.exists() {
        return Ok(f);
    }
    // else find file with same filestem
    let filestem = path.file_stem().context("get filestem failed")?;
    let dir = path.parent().context("get parent failed")?;
    for entry in dir.read_dir()? {
        let entry = entry?;
        let candidate = entry.path();
        if !candidate.is_file() {
            continue;
        }
        if candidate == path {
            continue;
        }
        if candidate.file_stem() == Some(filestem) {
            return Ok(candidate);
        }
    }

    bail!("video not found")
}

fn main() -> Result<()> {
    let f = std::env::args().nth(1).context("Missing input")?;
    let f = std::path::Path::new(&f);
    let source = find_video(f)?;

    let meta = std::fs::metadata(&source)?;
    filetime::set_file_mtime(f, FileTime::from_last_modification_time(&meta))?;

    Ok(())
}
