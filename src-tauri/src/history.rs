use crate::active_app_context::ActiveAppContextSnapshot;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::RwLock;
use tempfile::NamedTempFile;
use uuid::Uuid;

const MAX_HISTORY_ENTRIES: usize = 500;

/// Strategy for importing history entries
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryImportStrategy {
    /// Replace all existing entries with imported ones
    Replace,
    /// Append imported entries to existing ones (imported entries first/newer)
    MergeAppend,
    /// Merge but skip entries with matching IDs
    MergeDeduplicate,
}

/// Result of a history import operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryImportResult {
    pub success: bool,
    pub entries_imported: Option<usize>,
    pub entries_skipped: Option<usize>,
}

/// Outcome of the transcription that produced a history entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    #[default]
    Ok,
    Failed,
    /// Transcription (or re-transcription) is currently in flight
    Processing,
}

/// A single dictation history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub text: String,
    #[serde(default)]
    pub raw_text: String,
    #[serde(default)]
    pub active_app_context: Option<ActiveAppContextSnapshot>,
    #[serde(default)]
    pub status: TranscriptionStatus,
    #[serde(default)]
    pub error: Option<String>,
}

impl HistoryEntry {
    pub fn new(
        text: String,
        raw_text: String,
        active_app_context: Option<ActiveAppContextSnapshot>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            text,
            raw_text,
            active_app_context,
            status: TranscriptionStatus::Ok,
            error: None,
        }
    }
}

/// Storage for dictation history entries
#[derive(Debug, Serialize, Deserialize, Default)]
struct HistoryData {
    entries: Vec<HistoryEntry>,
}

/// Manages loading and saving of dictation history
pub struct HistoryStorage {
    data: RwLock<HistoryData>,
    file_path: PathBuf,
}

impl HistoryStorage {
    /// Create a new history storage with the given app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        let file_path = app_data_dir.join("history.json");

        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let mut data = match Self::load_from_file(&file_path) {
            Ok(history_data) => history_data,
            Err(error) => {
                if file_path.exists() {
                    log::warn!(
                        "Failed to load history from {}: {error}",
                        file_path.display()
                    );
                }
                HistoryData::default()
            }
        };

        // Entries stuck in Processing (app was killed mid-pipeline) become Failed
        for entry in &mut data.entries {
            if entry.status == TranscriptionStatus::Processing {
                entry.status = TranscriptionStatus::Failed;
                entry.error = Some("Interrupted (app restarted)".to_string());
            }
        }

        Self {
            data: RwLock::new(data),
            file_path,
        }
    }

    /// Load history from the JSON file
    fn load_from_file(file_path: &Path) -> Result<HistoryData> {
        let file_content = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read history file {}", file_path.display()))?;

        serde_json::from_str(&file_content)
            .with_context(|| format!("Failed to parse history file {}", file_path.display()))
    }

    /// Save current history to disk
    fn save(&self) -> Result<()> {
        let history_data = self.data.read().map_err(|error| {
            anyhow::anyhow!("Failed to acquire history read lock for save: {error}")
        })?;

        let serialized_history_content = serde_json::to_string_pretty(&*history_data)
            .context("Failed to serialize history data to JSON")?;

        let history_directory_path = self
            .file_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("History file path has no parent directory"))?;

        let mut temporary_history_file = NamedTempFile::new_in(history_directory_path)
            .with_context(|| {
                format!(
                    "Failed to create temporary history file in {}",
                    history_directory_path.display()
                )
            })?;

        temporary_history_file
            .write_all(serialized_history_content.as_bytes())
            .with_context(|| {
                format!(
                    "Failed to write temporary history file for {}",
                    self.file_path.display()
                )
            })?;

        temporary_history_file
            .as_file()
            .sync_all()
            .with_context(|| {
                format!(
                    "Failed to sync temporary history file for {}",
                    self.file_path.display()
                )
            })?;

        let persisted_history_file = temporary_history_file
            .persist(&self.file_path)
            .map_err(|persist_error| persist_error.error)
            .with_context(|| {
                format!(
                    "Failed to atomically replace history file {}",
                    self.file_path.display()
                )
            })?;

        persisted_history_file.sync_all().with_context(|| {
            format!(
                "Failed to sync persisted history file {}",
                self.file_path.display()
            )
        })?;

        Ok(())
    }

    /// Add a new entry to the history
    pub fn add_entry(
        &self,
        text: String,
        raw_text: String,
        active_app_context: Option<ActiveAppContextSnapshot>,
    ) -> Result<HistoryEntry> {
        let new_history_entry = HistoryEntry::new(text, raw_text, active_app_context);
        self.insert(new_history_entry.clone())?;
        Ok(new_history_entry)
    }

    /// Add a placeholder entry for a recording that is being transcribed
    pub fn add_processing_entry(&self) -> Result<HistoryEntry> {
        let mut processing_entry = HistoryEntry::new(String::new(), String::new(), None);
        processing_entry.status = TranscriptionStatus::Processing;
        self.insert(processing_entry.clone())?;
        Ok(processing_entry)
    }

    /// Insert an entry at the top of the history and persist
    fn insert(&self, entry: HistoryEntry) -> Result<()> {
        {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!("Failed to acquire history write lock when adding entry: {error}")
            })?;

            history_data.entries.insert(0, entry);

            if history_data.entries.len() > MAX_HISTORY_ENTRIES {
                history_data.entries.truncate(MAX_HISTORY_ENTRIES);
            }
        }
        self.save()
    }

    /// Replace an entry's transcription after a successful re-transcribe.
    /// Returns false if the entry no longer exists.
    pub fn update_entry_transcription(
        &self,
        id: &str,
        text: String,
        raw_text: String,
    ) -> Result<bool> {
        let updated = {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when updating entry {id}: {error}"
                )
            })?;

            if let Some(entry) = history_data.entries.iter_mut().find(|entry| entry.id == id) {
                entry.text = text;
                entry.raw_text = raw_text;
                entry.status = TranscriptionStatus::Ok;
                entry.error = None;
                true
            } else {
                false
            }
        };

        if updated {
            self.save()?;
        }

        Ok(updated)
    }

    /// Record a failed transcription attempt on an entry.
    /// An entry without successful text becomes `Failed`; an entry that already
    /// holds good text keeps it (status returns to `Ok`) and only stores the
    /// error as a hint — a failed re-transcribe must not wipe previous results.
    /// Returns false if the entry no longer exists.
    pub fn mark_entry_failed(&self, id: &str, error: String) -> Result<bool> {
        let changed = {
            let mut history_data = self.data.write().map_err(|err| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when marking entry {id} failed: {err}"
                )
            })?;

            match history_data
                .entries
                .iter_mut()
                .find(|entry| entry.id == id)
            {
                Some(entry) => {
                    entry.status = if entry.text.is_empty() {
                        TranscriptionStatus::Failed
                    } else {
                        TranscriptionStatus::Ok
                    };
                    entry.error = Some(error);
                    true
                }
                None => false,
            }
        };

        if changed {
            self.save()?;
        }

        Ok(changed)
    }

    /// Put an entry into the Processing state (re-transcription started).
    /// Existing text is kept so it stays visible while the retry runs.
    /// Returns false if the entry no longer exists.
    pub fn set_entry_processing(&self, id: &str) -> Result<bool> {
        let changed = {
            let mut history_data = self.data.write().map_err(|err| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when setting entry {id} processing: {err}"
                )
            })?;

            match history_data
                .entries
                .iter_mut()
                .find(|entry| entry.id == id)
            {
                Some(entry) => {
                    entry.status = TranscriptionStatus::Processing;
                    entry.error = None;
                    true
                }
                None => false,
            }
        };

        if changed {
            self.save()?;
        }

        Ok(changed)
    }

    /// Delete an entry only if it is still in the Processing state (pipeline cancel).
    /// Returns true if the entry was deleted.
    pub fn delete_entry_if_processing(&self, id: &str) -> Result<bool> {
        let deleted = {
            let mut history_data = self.data.write().map_err(|err| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when cancelling entry {id}: {err}"
                )
            })?;

            let initial_entry_count = history_data.entries.len();
            history_data
                .entries
                .retain(|entry| {
                    entry.id != id || entry.status != TranscriptionStatus::Processing
                });
            history_data.entries.len() < initial_entry_count
        };

        if deleted {
            self.save()?;
        }

        Ok(deleted)
    }

    /// Get all history entries (newest first), optionally limited
    pub fn get_all(&self, limit: Option<usize>) -> Result<Vec<HistoryEntry>> {
        let history_data = self.data.read().map_err(|error| {
            anyhow::anyhow!("Failed to acquire history read lock when getting entries: {error}")
        })?;

        let history_entries = match limit {
            Some(entry_limit) => history_data
                .entries
                .iter()
                .take(entry_limit)
                .cloned()
                .collect(),
            None => history_data.entries.clone(),
        };

        Ok(history_entries)
    }

    /// Delete an entry by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let deleted = {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when deleting entry {id}: {error}"
                )
            })?;

            let initial_entry_count = history_data.entries.len();
            history_data.entries.retain(|entry| entry.id != id);
            history_data.entries.len() < initial_entry_count
        };

        if deleted {
            self.save()?;
        }

        Ok(deleted)
    }

    /// Clear all history
    pub fn clear(&self) -> Result<()> {
        {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when clearing history: {error}"
                )
            })?;
            history_data.entries.clear();
        }
        self.save()
    }

    /// Import entries with the specified strategy
    pub fn import_entries(
        &self,
        mut entries: Vec<HistoryEntry>,
        strategy: HistoryImportStrategy,
    ) -> Result<HistoryImportResult> {
        let imported_count;
        let skipped_count;

        {
            let mut history_data = self.data.write().map_err(|error| {
                anyhow::anyhow!(
                    "Failed to acquire history write lock when importing entries: {error}"
                )
            })?;

            match strategy {
                HistoryImportStrategy::Replace => {
                    // Sort imported entries by timestamp (newest first)
                    entries.sort_by(|left_entry, right_entry| {
                        right_entry.timestamp.cmp(&left_entry.timestamp)
                    });
                    imported_count = entries.len();
                    skipped_count = 0;
                    history_data.entries = entries;
                }
                HistoryImportStrategy::MergeAppend => {
                    // Prepend imported entries (imported are considered newer)
                    // Sort imported entries by timestamp (newest first)
                    entries.sort_by(|left_entry, right_entry| {
                        right_entry.timestamp.cmp(&left_entry.timestamp)
                    });
                    imported_count = entries.len();
                    skipped_count = 0;

                    // Prepend imported entries to existing
                    let mut combined_entries = entries;
                    combined_entries.append(&mut history_data.entries);
                    history_data.entries = combined_entries;
                }
                HistoryImportStrategy::MergeDeduplicate => {
                    // Collect existing IDs
                    let existing_entry_ids: HashSet<String> = history_data
                        .entries
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect();

                    // Filter out entries that already exist
                    let new_entries: Vec<HistoryEntry> = entries
                        .into_iter()
                        .filter(|entry| !existing_entry_ids.contains(&entry.id))
                        .collect();

                    imported_count = new_entries.len();
                    skipped_count = 0; // We'll calculate this from the original count

                    // Prepend new entries
                    let mut combined_entries = new_entries;
                    combined_entries.append(&mut history_data.entries);

                    // Sort by timestamp (newest first)
                    combined_entries.sort_by(|left_entry, right_entry| {
                        right_entry.timestamp.cmp(&left_entry.timestamp)
                    });
                    history_data.entries = combined_entries;
                }
            }

            // Truncate to max entries
            if history_data.entries.len() > MAX_HISTORY_ENTRIES {
                history_data.entries.truncate(MAX_HISTORY_ENTRIES);
            }
        }

        self.save()?;

        Ok(HistoryImportResult {
            success: true,
            entries_imported: Some(imported_count),
            entries_skipped: Some(skipped_count),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_app_context::{
        FocusConfidenceLevel, FocusEventSource, FocusedApplication, FocusedBrowserTab,
        FocusedWindow,
    };

    struct TemporaryHistoryDirectory {
        path: PathBuf,
    }

    impl TemporaryHistoryDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("tambourine-history-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("failed to create temporary history test directory");
            Self { path }
        }
    }

    impl Drop for TemporaryHistoryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn loading_legacy_history_entries_defaults_optional_fields() {
        let temporary_history_directory = TemporaryHistoryDirectory::new();
        let history_file_path = temporary_history_directory.path.join("history.json");

        let legacy_history_content = r#"{
  "entries": [
    {
      "id": "legacy-entry-id",
      "timestamp": "2026-02-08T12:00:00Z",
      "text": "Legacy formatted text"
    }
  ]
}"#;

        fs::write(&history_file_path, legacy_history_content)
            .expect("failed to seed legacy history file");

        let history_storage = HistoryStorage::new(temporary_history_directory.path.clone());
        let loaded_entries = history_storage
            .get_all(None)
            .expect("failed to load legacy history entries");

        assert_eq!(loaded_entries.len(), 1);
        assert_eq!(loaded_entries[0].id, "legacy-entry-id");
        assert_eq!(loaded_entries[0].raw_text, "");
        assert!(loaded_entries[0].active_app_context.is_none());
    }

    #[test]
    fn entry_lifecycle_supports_retranscription() {
        let temporary_history_directory = TemporaryHistoryDirectory::new();
        let history_storage = HistoryStorage::new(temporary_history_directory.path.clone());

        // Recording stops → entry appears immediately as Processing
        let entry = history_storage
            .add_processing_entry()
            .expect("failed to add processing entry");
        assert_eq!(entry.status, TranscriptionStatus::Processing);
        assert!(entry.text.is_empty());

        // Pipeline fails → entry becomes Failed with the error text
        let marked = history_storage
            .mark_entry_failed(&entry.id, "STT unreachable".to_string())
            .expect("failed to mark entry failed");
        assert!(marked);
        let entries = history_storage.get_all(None).expect("failed to load");
        assert_eq!(entries[0].status, TranscriptionStatus::Failed);
        assert_eq!(entries[0].error.as_deref(), Some("STT unreachable"));

        // Re-transcribe starts → Processing again, error cleared
        let set_processing = history_storage
            .set_entry_processing(&entry.id)
            .expect("failed to set entry processing");
        assert!(set_processing);
        let entries = history_storage.get_all(None).expect("failed to load");
        assert_eq!(entries[0].status, TranscriptionStatus::Processing);
        assert!(entries[0].error.is_none());

        // Re-transcribe succeeds → Ok with text
        let updated = history_storage
            .update_entry_transcription(&entry.id, "Formatted".to_string(), "Raw".to_string())
            .expect("failed to update entry transcription");
        assert!(updated);
        let entries = history_storage.get_all(None).expect("failed to load");
        assert_eq!(entries[0].status, TranscriptionStatus::Ok);
        assert_eq!(entries[0].text, "Formatted");
        assert!(entries[0].error.is_none());

        // A later failed re-transcribe keeps the good text, stores the error hint
        let marked_again = history_storage
            .mark_entry_failed(&entry.id, "Timeout".to_string())
            .expect("failed to mark entry failed");
        assert!(marked_again);
        let entries = history_storage.get_all(None).expect("failed to load");
        assert_eq!(entries[0].status, TranscriptionStatus::Ok);
        assert_eq!(entries[0].text, "Formatted");
        assert_eq!(entries[0].error.as_deref(), Some("Timeout"));

        // Cancel deletes an entry only while it is Processing
        let not_deleted = history_storage
            .delete_entry_if_processing(&entry.id)
            .expect("failed to call cancel delete");
        assert!(!not_deleted);
        history_storage
            .set_entry_processing(&entry.id)
            .expect("failed to set entry processing");
        let deleted = history_storage
            .delete_entry_if_processing(&entry.id)
            .expect("failed to cancel-delete processing entry");
        assert!(deleted);
        assert!(history_storage.get_all(None).expect("failed to load").is_empty());

        // Updating a deleted entry reports false
        let updated_missing = history_storage
            .update_entry_transcription(&entry.id, String::new(), String::new())
            .expect("failed to call update on missing entry");
        assert!(!updated_missing);
    }

    #[test]
    fn stale_processing_entries_become_failed_on_load() {
        let temporary_history_directory = TemporaryHistoryDirectory::new();
        {
            let history_storage = HistoryStorage::new(temporary_history_directory.path.clone());
            history_storage
                .add_processing_entry()
                .expect("failed to add processing entry");
        }

        // Simulates an app restart while a pipeline was in flight
        let reloaded_storage = HistoryStorage::new(temporary_history_directory.path.clone());
        let entries = reloaded_storage.get_all(None).expect("failed to load");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, TranscriptionStatus::Failed);
        assert_eq!(
            entries[0].error.as_deref(),
            Some("Interrupted (app restarted)")
        );
    }

    #[test]
    fn add_entry_persists_active_app_context() {
        let temporary_history_directory = TemporaryHistoryDirectory::new();
        let history_storage = HistoryStorage::new(temporary_history_directory.path.clone());

        let active_app_context_snapshot = ActiveAppContextSnapshot {
            focused_application: Some(FocusedApplication {
                display_name: "Code".to_string(),
                bundle_id: Some("com.microsoft.VSCode".to_string()),
                process_path: None,
            }),
            focused_window: Some(FocusedWindow {
                title: "notes.md".to_string(),
            }),
            focused_browser_tab: Some(FocusedBrowserTab {
                title: Some("Issue tracker".to_string()),
                origin: Some("https://github.com".to_string()),
                browser: Some("Google Chrome".to_string()),
            }),
            event_source: FocusEventSource::Accessibility,
            confidence_level: FocusConfidenceLevel::High,
            captured_at: "2026-02-08T12:00:00Z".to_string(),
        };

        let new_history_entry = history_storage
            .add_entry(
                "Formatted text".to_string(),
                "Raw text".to_string(),
                Some(active_app_context_snapshot.clone()),
            )
            .expect("failed to add history entry with active app context");

        assert_eq!(
            new_history_entry.active_app_context,
            Some(active_app_context_snapshot.clone())
        );

        let persisted_entries = history_storage
            .get_all(Some(1))
            .expect("failed to read persisted history entry");

        assert_eq!(persisted_entries.len(), 1);
        assert_eq!(persisted_entries[0].text, "Formatted text");
        assert_eq!(persisted_entries[0].raw_text, "Raw text");
        assert_eq!(
            persisted_entries[0].active_app_context,
            Some(active_app_context_snapshot)
        );
    }
}
