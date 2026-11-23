//! Services for Witcher3StringEditor:
//! AppPaths, BackupService, RecentFilesService, LogAccessService, ShellOpenService.
#![forbid(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Local;
pub use settings_rs::port::{BackupItem, RecentItem, Settings, Timestamp};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// AppPaths
// ---------------------------------------------------------------------------

pub struct AppPaths;

impl AppPaths {
    pub fn data_directory() -> PathBuf {
        if let Ok(val) = std::env::var("W3_DATA_DIR") {
            return PathBuf::from(val);
        }
        if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("Witcher3StringEditor")
        } else {
            PathBuf::from(".data")
        }
    }

    pub fn backup_directory() -> PathBuf {
        Self::data_directory().join("Backups")
    }

    pub fn log_directory() -> PathBuf {
        Self::data_directory().join("Logs")
    }

    pub fn dictionary_directory() -> PathBuf {
        Self::data_directory().join("Dictionaries")
    }

    pub fn settings_path() -> PathBuf {
        Self::data_directory().join("settings.json")
    }
}

// ---------------------------------------------------------------------------
// Timestamp helper
// ---------------------------------------------------------------------------

pub fn current_timestamp() -> Timestamp {
    let now = Local::now();
    let dt_str = now.format("%Y-%m-%dT%H:%M:%S").to_string();
    let nanos = now.timestamp_subsec_nanos();
    let ticks = nanos / 100;
    Timestamp {
        date_time: dt_str,
        ticks,
        utc: false,
    }
}

// ---------------------------------------------------------------------------
// BackupService
// ---------------------------------------------------------------------------

pub struct BackupService;

impl BackupService {
    pub fn compute_sha256(path: &Path) -> Result<String, String> {
        let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let result = hasher.finalize();
        Ok(format!("{result:x}"))
    }

    pub fn backup(file_path: &Path, settings: &mut Settings) -> Result<bool, String> {
        if !file_path.exists() {
            return Err(format!("File does not exist: {}", file_path.display()));
        }

        let hash = Self::compute_sha256(file_path)?;
        let origin_str = file_path.to_string_lossy().to_string();

        let is_dup = settings.backup_items.iter().any(|item| {
            item.hash == hash
                && item.orgin_path == origin_str
                && Path::new(&item.backup_path).exists()
        });

        if is_dup {
            return Ok(false);
        }

        let backup_dir = AppPaths::backup_directory();
        fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;

        let backup_filename = format!("{}.bak", Uuid::new_v4().simple());
        let backup_path = backup_dir.join(&backup_filename);

        fs::copy(file_path, &backup_path).map_err(|e| e.to_string())?;

        let file_name = file_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "backup".to_string());

        let item = BackupItem {
            file_name,
            hash,
            orgin_path: origin_str,
            backup_path: backup_path.to_string_lossy().to_string(),
            backup_time: current_timestamp(),
        };

        settings.backup_items.insert(0, item);
        Ok(true)
    }

    pub fn restore(item: &BackupItem) -> Result<(), String> {
        let src = Path::new(&item.backup_path);
        let dst = Path::new(&item.orgin_path);

        if !src.exists() {
            return Err(format!("Backup file not found: {}", src.display()));
        }

        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        fs::copy(src, dst).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(item: &BackupItem, settings: &mut Settings) -> Result<(), String> {
        let path = Path::new(&item.backup_path);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        if let Some(pos) = settings
            .backup_items
            .iter()
            .position(|i| i.backup_path == item.backup_path)
        {
            settings.backup_items.remove(pos);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RecentFilesService
// ---------------------------------------------------------------------------

pub struct RecentFilesService;

impl RecentFilesService {
    pub const MAX_RECENT: usize = 50;

    pub fn add_or_update(file_path: &str, settings: &mut Settings) {
        if file_path.trim().is_empty() {
            return;
        }

        let ts = current_timestamp();

        if let Some(pos) = settings
            .recent_items
            .iter()
            .position(|r| r.file_path == file_path)
        {
            let mut item = settings.recent_items.remove(pos);
            item.opened_time = ts;
            settings.recent_items.insert(0, item);
        } else {
            let item = RecentItem {
                file_path: file_path.to_string(),
                is_marked: false,
                opened_time: ts,
            };
            settings.recent_items.insert(0, item);
            if settings.recent_items.len() > Self::MAX_RECENT {
                settings.recent_items.truncate(Self::MAX_RECENT);
            }
        }
    }

    pub fn remove(file_path: &str, settings: &mut Settings) {
        if let Some(pos) = settings
            .recent_items
            .iter()
            .position(|r| r.file_path == file_path)
        {
            settings.recent_items.remove(pos);
        }
    }

    pub fn toggle_mark(file_path: &str, settings: &mut Settings) {
        if let Some(item) = settings
            .recent_items
            .iter_mut()
            .find(|r| r.file_path == file_path)
        {
            item.is_marked = !item.is_marked;
        }
    }
}

// ---------------------------------------------------------------------------
// LogAccessService
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEvent {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone)]
pub struct LogAccessService {
    logs: Arc<Mutex<Vec<LogEvent>>>,
    max_logs: usize,
}

impl Default for LogAccessService {
    fn default() -> Self {
        Self::new(5000)
    }
}

impl LogAccessService {
    pub fn new(max_logs: usize) -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::with_capacity(max_logs))),
            max_logs,
        }
    }

    pub fn add(&self, level: &str, message: &str) {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let event = LogEvent {
            timestamp: now,
            level: level.to_string(),
            message: message.to_string(),
        };

        let log_dir = AppPaths::log_directory();
        if fs::create_dir_all(&log_dir).is_ok() {
            let log_file = log_dir.join("log.txt");
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
                let _ = writeln!(f, "[{} {}] {}", event.timestamp, event.level, event.message);
            }
        }

        if let Ok(mut list) = self.logs.lock() {
            list.push(event);
            if list.len() > self.max_logs {
                let excess = list.len() - self.max_logs;
                list.drain(0..excess);
            }
        }
    }

    pub fn snapshot(&self) -> Vec<LogEvent> {
        self.logs.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut list) = self.logs.lock() {
            list.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// ShellOpenService
// ---------------------------------------------------------------------------

pub struct ShellOpenService;

impl ShellOpenService {
    pub fn open(path_or_url: &str) -> Result<(), String> {
        open::that(path_or_url).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// SettingsPersistenceService
// ---------------------------------------------------------------------------

pub struct SettingsPersistenceService;

impl SettingsPersistenceService {
    pub fn load() -> Settings {
        let path = AppPaths::settings_path();
        if let Ok(content) = fs::read_to_string(&path)
            && let Ok(settings) = settings_rs::port::parse(&content)
        {
            return settings;
        }
        settings_rs::port::default_settings()
    }

    pub fn save(settings: &Settings) -> Result<(), String> {
        let path = AppPaths::settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = settings_rs::port::serialize(settings);
        fs::write(&path, json.as_bytes()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recent_files() {
        let mut s = settings_rs::port::default_settings();
        RecentFilesService::add_or_update("/path/to/1.w3strings", &mut s);
        RecentFilesService::add_or_update("/path/to/2.w3strings", &mut s);
        assert_eq!(s.recent_items.len(), 2);
        assert_eq!(s.recent_items[0].file_path, "/path/to/2.w3strings");

        RecentFilesService::toggle_mark("/path/to/2.w3strings", &mut s);
        assert!(s.recent_items[0].is_marked);

        RecentFilesService::remove("/path/to/1.w3strings", &mut s);
        assert_eq!(s.recent_items.len(), 1);
    }

    #[test]
    fn test_log_service() {
        let log = LogAccessService::new(10);
        log.add("INFO", "App started");
        log.add("WARN", "Warning message");
        let snaps = log.snapshot();
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].message, "App started");
    }
}
