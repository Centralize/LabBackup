//! Saved settings: backup sets and the history log, stored as JSON in the
//! platform's config folder (AppData on Windows, ~/Library on macOS,
//! ~/.config on Linux).

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupJob {
    pub id: u64,
    pub name: String,
    pub sources: Vec<PathBuf>,
    pub destination: PathBuf,
    /// Wildcard patterns matched against file and folder names, e.g. `*.tmp`, `.git`.
    pub excludes: Vec<String>,
    /// How many backups to keep for this set; 0 keeps all of them.
    pub keep_last: u32,
    pub auto_enabled: bool,
    pub auto_every_hours: u32,
    pub compress: bool,
    pub encrypt: bool,
    pub last_run: Option<DateTime<Local>>,
}

impl Default for BackupJob {
    fn default() -> Self {
        Self {
            id: 0,
            name: "New backup set".into(),
            sources: Vec::new(),
            destination: default_destination(),
            excludes: vec!["*.tmp".into(), "~$*".into(), ".DS_Store".into(), "Thumbs.db".into()],
            keep_last: 10,
            auto_enabled: false,
            auto_every_hours: 24,
            compress: true,
            encrypt: false,
            last_run: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryKind {
    Backup,
    Restore,
    Verify,
}

impl HistoryKind {
    pub fn label(self) -> &'static str {
        match self {
            HistoryKind::Backup => "Backup",
            HistoryKind::Restore => "Restore",
            HistoryKind::Verify => "Verify",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub time: DateTime<Local>,
    pub kind: HistoryKind,
    pub job: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub jobs: Vec<BackupJob>,
    pub history: Vec<HistoryEntry>,
    pub dark_mode: bool,
    pub next_id: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self { jobs: Vec::new(), history: Vec::new(), dark_mode: true, next_id: 1 }
    }
}

pub fn default_destination() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().join("LabBackup Backups"))
        .unwrap_or_else(|| PathBuf::from("LabBackup Backups"))
}

pub fn default_source() -> Option<PathBuf> {
    directories::UserDirs::new().and_then(|u| u.document_dir().map(|p| p.to_path_buf()))
}

fn config_path() -> Option<PathBuf> {
    // A different settings file can be used for testing or portable installs.
    if let Some(p) = std::env::var_os("LABBACKUP_CONFIG") {
        return Some(PathBuf::from(p));
    }
    directories::ProjectDirs::from("org", "LabBackup", "LabBackup")
        .map(|d| d.config_dir().join("config.json"))
}

impl Config {
    pub fn load() -> Self {
        let mut cfg: Config = config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if cfg.jobs.is_empty() {
            let mut job = BackupJob { id: cfg.next_id, name: "My Documents".into(), ..Default::default() };
            cfg.next_id += 1;
            if let Some(docs) = default_source() {
                job.sources.push(docs);
            }
            cfg.jobs.push(job);
        }
        cfg.next_id = cfg.next_id.max(cfg.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1);
        cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path().ok_or("No config folder on this system")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn log(&mut self, kind: HistoryKind, job: &str, ok: bool, message: impl Into<String>) {
        self.history.insert(
            0,
            HistoryEntry { time: Local::now(), kind, job: job.to_string(), ok, message: message.into() },
        );
        self.history.truncate(500);
    }
}
