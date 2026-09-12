use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lumasync-diagnostics-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn drain(diagnostics: &Diagnostics) {
    let writer = diagnostics.buffer.lock().unwrap().writer.clone().unwrap();
    let (sender, receiver) = mpsc::channel();
    writer.send(WriteCommand::Flush(sender)).unwrap();
    receiver.recv_timeout(Duration::from_secs(3)).unwrap();
}

#[test]
fn secrets_and_credential_urls_are_redacted_before_storage() {
    let message = r#"request https://192.168.1.2/api/secret/lights failed {"appKey":"abc","client_key":"def"} Authorization: Bearer short-secret"#;
    let clean = redaction::sanitize(message);
    for secret in ["192.168.1.2", "secret/lights", "abc", "def", "short-secret"] {
        assert!(!clean.contains(secret), "leaked {secret}: {clean}");
    }
    assert!(!redaction::sanitize("key 0123456789abcdef0123456789abcdef").contains("01234567"));
    assert_eq!(redaction::sanitize(&"é".repeat(2000)).chars().count(), 1500);
}

#[test]
fn memory_retains_only_the_latest_entries() {
    let diagnostics = Diagnostics::default();
    for index in 0..MAX_ENTRIES + 10 {
        diagnostics.record(Level::Info, "test", &index.to_string());
    }
    let buffer = diagnostics.buffer.lock().unwrap();
    assert_eq!(buffer.entries.len(), MAX_ENTRIES);
    assert_eq!(buffer.entries.front().unwrap().message, "10");
    assert_eq!(buffer.entries.back().unwrap().message, "209");
}

#[test]
fn entries_survive_restart_and_clear_removes_disk_history() {
    let directory = TestDirectory::new();
    let diagnostics = Arc::new(Diagnostics::default());
    diagnostics.initialize(directory.0.clone()).unwrap();
    diagnostics.record(Level::Error, "capture.failed", "Capture unavailable.");
    drain(&diagnostics);
    let path = directory.0.join("diagnostics.jsonl");
    assert_eq!(storage::read_entries(&path).unwrap().len(), 1);
    drop(diagnostics);
    let restored = Arc::new(Diagnostics::default());
    restored.initialize(directory.0.clone()).unwrap();
    assert_eq!(
        restored
            .buffer
            .lock()
            .unwrap()
            .entries
            .back()
            .unwrap()
            .component,
        "capture.failed"
    );
    restored.clear().unwrap();
    assert!(storage::read_entries(&path).unwrap().is_empty());
    assert!(restored.buffer.lock().unwrap().entries.is_empty());
}

#[test]
fn rotation_bounds_both_files_and_preserves_newest_entries() {
    let directory = TestDirectory::new();
    let path = directory.0.join("diagnostics.jsonl");
    let mut entry = storage_warning(&"x ".repeat(750));
    for index in 0..1000 {
        entry.timestamp_ms = index;
        storage::append(&path, &entry).unwrap();
    }
    assert!(fs::metadata(&path).unwrap().len() <= storage::MAX_FILE_BYTES);
    assert!(fs::metadata(storage::backup_path(&path)).unwrap().len() <= storage::MAX_FILE_BYTES);
    let entries = storage::read_entries(&path).unwrap();
    assert_eq!(entries.len(), MAX_ENTRIES);
    assert_eq!(entries.back().unwrap().timestamp_ms, 999);
    storage::clear(&path).unwrap();
    assert!(!storage::backup_path(&path).exists());
}

#[test]
fn a_full_writer_queue_never_blocks_diagnostic_producers() {
    let diagnostics = Diagnostics::default();
    let (writer, _receiver) = mpsc::sync_channel(1);
    diagnostics.buffer.lock().unwrap().writer = Some(writer);
    for _ in 0..1000 {
        diagnostics.record(Level::Error, "test", "Failure");
    }
    assert_eq!(
        diagnostics.buffer.lock().unwrap().entries.len(),
        MAX_ENTRIES
    );
    assert!(diagnostics.clear().is_err());
}

#[test]
fn ipc_report_uses_the_frontend_contract() {
    let report = DiagnosticReport {
        entries: vec![storage_warning("Failure")],
        log_path: None,
    };
    let value = serde_json::to_value(report).unwrap();
    assert!(value["logPath"].is_null());
    assert!(value["entries"][0]["timestampMs"].is_u64());
    assert_eq!(value["entries"][0]["level"], "warn");
    assert_eq!(value["entries"][0]["component"], "diagnostics.storage");
}

#[test]
fn an_interrupted_file_write_does_not_disable_persistent_diagnostics() {
    use std::io::Write;
    let directory = TestDirectory::new();
    let path = directory.0.join("diagnostics.jsonl");
    storage::append(&path, &storage_warning("Before interruption")).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"message\":\"\xc3")
        .unwrap();
    let diagnostics = Arc::new(Diagnostics::default());
    diagnostics.initialize(directory.0.clone()).unwrap();
    assert_eq!(diagnostics.buffer.lock().unwrap().entries.len(), 1);
    diagnostics.record(Level::Info, "application.start", "After interruption");
    drain(&diagnostics);
    assert_eq!(storage::read_entries(&path).unwrap().len(), 2);
}
