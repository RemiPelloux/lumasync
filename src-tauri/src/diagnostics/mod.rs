mod redaction;
mod storage;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex, OnceLock, Weak,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_ENTRIES: usize = 200;
const WRITER_QUEUE: usize = 128;
const MAX_COMPONENT_CHARS: usize = 80;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub timestamp_ms: u64,
    pub level: Level,
    pub component: String,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub entries: Vec<DiagnosticEntry>,
    pub log_path: Option<String>,
}

#[derive(Default)]
struct Buffer {
    entries: VecDeque<DiagnosticEntry>,
    path: Option<PathBuf>,
    writer: Option<SyncSender<WriteCommand>>,
    queue_warning: bool,
}

enum WriteCommand {
    Entry(DiagnosticEntry),
    Clear(mpsc::Sender<Result<(), String>>),
    Flush(mpsc::Sender<()>),
}

#[derive(Default)]
struct Diagnostics {
    buffer: Mutex<Buffer>,
}

fn global() -> &'static Arc<Diagnostics> {
    static DIAGNOSTICS: OnceLock<Arc<Diagnostics>> = OnceLock::new();
    DIAGNOSTICS.get_or_init(|| Arc::new(Diagnostics::default()))
}

pub fn initialize(directory: Result<PathBuf, String>) {
    if let Err(error) = directory.and_then(|directory| global().initialize(directory)) {
        warn("diagnostics.storage", &error);
    }
    info(
        "application.start",
        "Application started; local diagnostics enabled.",
    );
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let location = panic.location().map_or_else(
            || "unknown location".to_owned(),
            |location| format!("{}:{}", location.file(), location.line()),
        );
        error(
            "application.panic",
            &format!("Unexpected panic at {location}."),
        );
        flush();
        previous(panic);
    }));
}

pub fn info(component: &str, message: &str) {
    global().record(Level::Info, component, message);
}
pub fn warn(component: &str, message: &str) {
    global().record(Level::Warn, component, message);
}
pub fn error(component: &str, message: &str) {
    global().record(Level::Error, component, message);
}

pub fn flush() {
    let writer = global()
        .buffer
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .writer
        .clone();
    if let Some(writer) = writer {
        let (sender, receiver) = mpsc::channel();
        if writer.try_send(WriteCommand::Flush(sender)).is_ok() {
            let _ = receiver.recv_timeout(Duration::from_secs(1));
        }
    }
}

#[tauri::command]
pub fn get_diagnostics() -> DiagnosticReport {
    let buffer = global()
        .buffer
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    DiagnosticReport {
        entries: buffer.entries.iter().cloned().collect(),
        log_path: buffer
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
    }
}

#[tauri::command]
pub async fn clear_diagnostics() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| global().clear())
        .await
        .map_err(|_| "Le nettoyage du journal a ete interrompu.".to_owned())?
}

#[tauri::command]
pub fn log_frontend_event(component: String, message: String, level: Option<Level>) {
    global().record(
        level.unwrap_or(Level::Error),
        &format!("frontend.{component}"),
        &message,
    );
}

impl Diagnostics {
    fn initialize(self: &Arc<Self>, directory: PathBuf) -> Result<(), String> {
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = directory.join("diagnostics.jsonl");
        storage::prepare(&path).map_err(|error| error.to_string())?;
        let entries = storage::read_entries(&path).map_err(|error| error.to_string())?;
        let (sender, receiver) = mpsc::sync_channel(WRITER_QUEUE);
        let (weak, target) = (Arc::downgrade(self), path.clone());
        std::thread::Builder::new()
            .name("lumasync-diagnostics".to_owned())
            .spawn(move || write_loop(weak, target, receiver))
            .map_err(|error| error.to_string())?;
        let mut buffer = self
            .buffer
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        buffer.entries = entries;
        buffer.path = Some(path);
        buffer.writer = Some(sender);
        Ok(())
    }

    fn record(&self, level: Level, component: &str, message: &str) {
        let entry = DiagnosticEntry {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            level,
            component: redaction::sanitize(component)
                .chars()
                .take(MAX_COMPONENT_CHARS)
                .collect(),
            message: redaction::sanitize(message),
        };
        let mut buffer = self
            .buffer
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        push(&mut buffer.entries, entry.clone());
        if let Some(writer) = &buffer.writer {
            if writer.try_send(WriteCommand::Entry(entry)).is_err() && !buffer.queue_warning {
                buffer.queue_warning = true;
                push(
                    &mut buffer.entries,
                    storage_warning("Diagnostic writer is busy; some file entries were skipped."),
                );
            }
        }
    }

    fn clear(&self) -> Result<(), String> {
        let receiver = {
            let mut buffer = self
                .buffer
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let receiver = if let Some(writer) = &buffer.writer {
                let (sender, receiver) = mpsc::channel();
                writer.try_send(WriteCommand::Clear(sender)).map_err(|_| {
                    "Le journal local est occupe; reessayez dans un instant.".to_owned()
                })?;
                Some(receiver)
            } else {
                if let Some(path) = &buffer.path {
                    storage::clear(path).map_err(|error| error.to_string())?;
                }
                None
            };
            buffer.entries.clear();
            buffer.queue_warning = false;
            receiver
        };
        receiver.map_or(Ok(()), |receiver| {
            receiver
                .recv_timeout(Duration::from_secs(3))
                .map_err(|_| "Le nettoyage du journal local a expire.".to_owned())?
        })
    }
}

fn push(entries: &mut VecDeque<DiagnosticEntry>, entry: DiagnosticEntry) {
    if entries.len() == MAX_ENTRIES {
        entries.pop_front();
    }
    entries.push_back(entry);
}

fn storage_warning(message: &str) -> DiagnosticEntry {
    DiagnosticEntry {
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        level: Level::Warn,
        component: "diagnostics.storage".to_owned(),
        message: message.to_owned(),
    }
}

fn write_loop(
    diagnostics: Weak<Diagnostics>,
    path: PathBuf,
    receiver: mpsc::Receiver<WriteCommand>,
) {
    for command in receiver {
        let result = match command {
            WriteCommand::Entry(entry) => storage::append(&path, &entry),
            WriteCommand::Clear(sender) => {
                let result = storage::clear(&path);
                let _ = sender.send(
                    result
                        .as_ref()
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                );
                result
            }
            WriteCommand::Flush(sender) => {
                let _ = sender.send(());
                Ok(())
            }
        };
        if result.is_err() {
            if let Some(diagnostics) = diagnostics.upgrade() {
                let mut buffer = diagnostics
                    .buffer
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                buffer.writer = None;
                push(
                    &mut buffer.entries,
                    storage_warning(
                        "Local log writing failed; diagnostics remain available in memory.",
                    ),
                );
            }
            break;
        }
    }
}
