use super::{DiagnosticEntry, MAX_ENTRIES};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub(super) const MAX_FILE_BYTES: u64 = 512 * 1024;

pub(super) fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("previous.jsonl")
}

pub(super) fn prepare(path: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(path)?;
    if file.metadata()?.len() > 0 {
        file.seek(SeekFrom::End(-1))?;
        let mut last = [0];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            file.write_all(b"\n")?;
        }
    }
    Ok(())
}

pub(super) fn read_entries(path: &Path) -> io::Result<VecDeque<DiagnosticEntry>> {
    let mut entries = VecDeque::with_capacity(MAX_ENTRIES);
    for source in [backup_path(path), path.to_path_buf()] {
        let file = match File::open(source) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for line in BufReader::new(file).lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) if error.kind() == io::ErrorKind::InvalidData => continue,
                Err(error) => return Err(error),
            };
            if let Ok(entry) = serde_json::from_str::<DiagnosticEntry>(&line) {
                if entries.len() == MAX_ENTRIES {
                    entries.pop_front();
                }
                entries.push_back(entry);
            }
        }
    }
    Ok(entries)
}

pub(super) fn append(path: &Path, entry: &DiagnosticEntry) -> io::Result<()> {
    let mut line = serde_json::to_vec(entry)?;
    line.push(b'\n');
    let current = fs::metadata(path).map_or(0, |metadata| metadata.len());
    if current + line.len() as u64 > MAX_FILE_BYTES {
        rotate(path)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&line)
}

fn rotate(path: &Path) -> io::Result<()> {
    match fs::remove_file(backup_path(path)) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    fs::rename(path, backup_path(path))
}

pub(super) fn clear(path: &Path) -> io::Result<()> {
    File::create(path)?;
    match fs::remove_file(backup_path(path)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
