//! Private subject-to-task references. Prompts and credentials are never stored.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::{config, state};

mod submission;
pub use submission::{SUBMISSION_TIMEOUT, Submission, SubmissionTimeout};

const MAX_SUBJECT_TASKS: usize = 50;
const MAX_SUBJECT_ID_CHARS: usize = 200;
const MAX_TASK_ID_CHARS: usize = 100;
/// The current file version. Version 1 had no `monitor` flag.
const FILE_VERSION: u32 = 2;
/// The directory mode of the task file's directory: owner only.
#[cfg(unix)]
const OWNER_ONLY_DIR_MODE: u32 = 0o700;
// Two dashboards share one file. Keep its read-modify-write operation atomic
// within the process, including across separately constructed store handles.
static WRITES: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
    pub repo: String,
    pub task_id: String,
    pub created_at: String,
    /// Whether a finished or stuck task raises an alert. Version 1 files have
    /// no flag; they read as monitored, which only affects active tasks
    /// because the watcher polls no others.
    #[serde(default = "monitored_by_default")]
    pub monitor: bool,
}

fn monitored_by_default() -> bool {
    true
}

/// One subject's reference, for a caller that works across subjects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectTaskRef {
    pub subject: String,
    pub task: TaskRef,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    subjects: BTreeMap<String, Vec<TaskRef>>,
}

impl Default for File {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            subjects: BTreeMap::new(),
        }
    }
}

// qual:api
#[derive(Clone)]
pub struct TaskStore {
    file: PathBuf,
}

impl TaskStore {
    pub fn from_system() -> Option<Self> {
        state::sibling("copilot-agent-tasks.json").map(Self::new)
    }

    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    fn read(&self) -> io::Result<File> {
        let text = match fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(File::default()),
            Err(error) => return Err(error),
        };
        let mut file: File = serde_json::from_str(&text).map_err(io::Error::other)?;
        if file.version == 0 || file.version > FILE_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unknown agent task file version.",
            ));
        }
        file.version = FILE_VERSION;
        file.subjects.retain(|subject, entries| {
            entries.retain(valid_entry);
            valid_subject(subject)
        });
        Ok(file)
    }

    pub fn tasks(&self, subject: &str) -> io::Result<Vec<TaskRef>> {
        if !valid_subject(subject) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid agent subject.",
            ));
        }
        let _guard = WRITES.lock().unwrap_or_else(PoisonError::into_inner);
        let mut entries = self.read()?.subjects.remove(subject).unwrap_or_default();
        entries.drain(..entries.len().saturating_sub(MAX_SUBJECT_TASKS));
        Ok(entries)
    }

    pub fn record(&self, subject: &str, entry: TaskRef) -> io::Result<()> {
        if !valid_subject(subject) || !valid_entry(&entry) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid agent task reference.",
            ));
        }
        let _guard = WRITES.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = self.read()?;
        let entries = file.subjects.entry(subject.to_owned()).or_default();
        entries.retain(|existing| {
            existing.task_id != entry.task_id || !existing.repo.eq_ignore_ascii_case(&entry.repo)
        });
        entries.push(entry);
        entries.drain(..entries.len().saturating_sub(MAX_SUBJECT_TASKS));
        self.save(&file)
    }

    /// Every reference the file holds, with the subject that owns it.
    pub fn all(&self) -> io::Result<Vec<SubjectTaskRef>> {
        let _guard = WRITES.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(flatten(self.read()?.subjects))
    }

    /// Turns alerts on or off for every reference to `task_id`. `false` means
    /// the file holds no such task.
    // qual:api
    pub fn set_monitor(&self, task_id: &str, monitor: bool) -> io::Result<bool> {
        let _guard = WRITES.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = self.read()?;
        if !apply_monitor(&mut file.subjects, task_id, monitor) {
            return Ok(false);
        }
        self.save(&file)?;
        Ok(true)
    }

    fn save(&self, file: &File) -> io::Result<()> {
        if let Some(dir) = self.file.parent() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(OWNER_ONLY_DIR_MODE);
            }
            builder.create(dir)?;
        }
        let text = serde_json::to_string_pretty(file).map_err(io::Error::other)?;
        state::write_atomic(&self.file, &text)
    }
}

/// The references of every subject, each paired with its subject key.
fn flatten(subjects: BTreeMap<String, Vec<TaskRef>>) -> Vec<SubjectTaskRef> {
    subjects
        .into_iter()
        .flat_map(|(subject, entries)| {
            entries.into_iter().map(move |task| SubjectTaskRef {
                subject: subject.clone(),
                task,
            })
        })
        .collect()
}

/// Sets the monitor flag of every matching reference; `false` means none did.
fn apply_monitor(
    subjects: &mut BTreeMap<String, Vec<TaskRef>>,
    task_id: &str,
    monitor: bool,
) -> bool {
    let mut found = false;
    for entry in subjects.values_mut().flatten() {
        if entry.task_id == task_id {
            entry.monitor = monitor;
            found = true;
        }
    }
    found
}

fn valid_subject(subject: &str) -> bool {
    let Some((kind, id)) = subject.split_once(':') else {
        return false;
    };
    matches!(kind, "chat" | "other")
        && (1..=MAX_SUBJECT_ID_CHARS).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

fn valid_entry(entry: &TaskRef) -> bool {
    config::is_repo(&entry.repo)
        && (1..=MAX_TASK_ID_CHARS).contains(&entry.task_id.len())
        && entry
            .task_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::thread;

    use super::{TaskRef, TaskStore};

    fn entry(id: &str) -> TaskRef {
        TaskRef {
            repo: "o/r".into(),
            task_id: id.into(),
            created_at: "2026-10-03T00:00:00Z".into(),
            monitor: true,
        }
    }

    #[test]
    fn references_are_deduplicated_capped_and_survive_reload() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("private").join("tasks.json");
        let store = TaskStore::new(file.clone());
        for id in 0..55 {
            store.record("other:issue", entry(&id.to_string())).unwrap();
        }
        store.record("other:issue", entry("54")).unwrap();
        let loaded = TaskStore::new(file.clone()).tasks("other:issue").unwrap();
        assert_eq!(loaded.len(), 50);
        assert_eq!(loaded[0].task_id, "5");
        assert_eq!(loaded[49].task_id, "54");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(file.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn separate_handles_do_not_lose_concurrent_records() {
        let dir = tempfile::tempdir().unwrap();
        let file = Arc::new(dir.path().join("tasks.json"));
        thread::scope(|scope| {
            for id in 0..20 {
                let file = file.clone();
                scope.spawn(move || {
                    TaskStore::new((*file).clone())
                        .record("other:risk", entry(&id.to_string()))
                        .unwrap()
                });
            }
        });
        assert_eq!(
            TaskStore::new((*file).clone())
                .tasks("other:risk")
                .unwrap()
                .len(),
            20
        );
    }

    #[test]
    fn broken_files_are_not_overwritten_and_invalid_references_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tasks.json");
        fs::write(&file, "{broken").unwrap();
        let store = TaskStore::new(file.clone());
        assert!(store.record("other:risk", entry("task")).is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), "{broken");
        assert!(store.tasks("bad:subject").is_err());
        assert!(store.record("other:risk", entry("../unsafe")).is_err());
    }

    #[test]
    fn version_1_files_load_as_monitored_and_save_as_version_2() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tasks.json");
        fs::write(
            &file,
            r#"{"version":1,"subjects":{"chat:task-1":[{"repo":"o/r","taskId":"task-1","createdAt":"2026-10-03T00:00:00Z"}]}}"#,
        )
        .unwrap();
        let store = TaskStore::new(file.clone());
        let loaded = store.tasks("chat:task-1").unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].monitor, "a version 1 reference is monitored");
        assert!(super::monitored_by_default());
        store.record("chat:task-1", entry("task-2")).unwrap();
        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("\"version\": 2")
        );
        fs::write(&file, r#"{"version":3,"subjects":{}}"#).unwrap();
        assert!(store.tasks("chat:task-1").is_err());
    }

    #[test]
    fn monitoring_toggles_by_task_id_and_every_subject_is_listed() {
        let dir = tempfile::tempdir().unwrap();
        let store = TaskStore::new(dir.path().join("tasks.json"));
        store.record("chat:task-1", entry("task-1")).unwrap();
        store.record("other:issue", entry("task-2")).unwrap();
        assert!(store.set_monitor("task-1", false).unwrap());
        assert!(!store.set_monitor("missing", false).unwrap());
        let all = store.all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].subject, "chat:task-1");
        assert!(!all[0].task.monitor);
        assert_eq!(all[1].subject, "other:issue");
        assert!(all[1].task.monitor);
        assert!(store.set_monitor("task-1", true).unwrap());
        assert!(store.tasks("chat:task-1").unwrap()[0].monitor);
    }
}
