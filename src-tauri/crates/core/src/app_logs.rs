use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, NaiveDate, Utc};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;

const ACCESS_LOG_NAME: &str = "access.log";
const ERROR_LOG_NAME: &str = "error.log";

#[derive(Debug, Clone, Copy)]
pub struct LogConfig {
    pub rotation_size_bytes: u64,
    pub max_rotations: usize,
    pub retention_days: u64,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            rotation_size_bytes: 10 * 1024 * 1024,
            max_rotations: 7,
            retention_days: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessLogEntry {
    pub ts: String,
    pub rule_id: String,
    pub client: String,
    pub protocol: String,
    pub method: String,
    pub target: String,
    pub status: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub duration_ms: u64,
}

impl AccessLogEntry {
    pub fn success(
        rule_id: impl Into<String>,
        client: impl Into<String>,
        protocol: impl Into<String>,
        method: impl Into<String>,
        target: impl Into<String>,
        bytes_in: u64,
        bytes_out: u64,
        duration_ms: u64,
    ) -> Self {
        Self {
            ts: iso_timestamp(Utc::now()),
            rule_id: rule_id.into(),
            client: client.into(),
            protocol: protocol.into(),
            method: method.into(),
            target: target.into(),
            status: "success".to_owned(),
            bytes_in,
            bytes_out,
            duration_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorLogEntry {
    pub ts: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub error_type: String,
    pub error_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

impl ErrorLogEntry {
    pub fn new(error_type: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self {
            ts: iso_timestamp(Utc::now()),
            rule_id: None,
            error_type: error_type.into(),
            error_message: error_message.into(),
            client: None,
            target: None,
            detail: None,
        }
    }

    pub fn with_rule_id(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    pub fn with_client(mut self, client: impl Into<String>) -> Self {
        self.client = Some(client.into());
        self
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_detail(mut self, detail: Value) -> Self {
        self.detail = Some(detail);
        self
    }
}

#[derive(Debug)]
pub struct AppLogger {
    sender: Option<Sender<LogCommand>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl AppLogger {
    pub fn disabled() -> Self {
        Self {
            sender: None,
            worker: Mutex::new(None),
        }
    }

    pub fn new(log_dir: impl AsRef<Path>) -> io::Result<Self> {
        Self::new_with_config(log_dir, LogConfig::default())
    }

    pub fn new_with_config(log_dir: impl AsRef<Path>, config: LogConfig) -> io::Result<Self> {
        let writers = WriterSet::new(log_dir.as_ref().to_path_buf(), config)?;
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut writers = writers;
            while let Ok(command) = receiver.recv() {
                let result = match command {
                    LogCommand::Access(entry) => writers.write_access(entry),
                    LogCommand::Error(entry) => writers.write_error(entry),
                };
                if let Err(err) = result {
                    eprintln!("app log write failed: {err}");
                }
            }
        });

        Ok(Self {
            sender: Some(sender),
            worker: Mutex::new(Some(worker)),
        })
    }

    pub fn log_access(&self, entry: AccessLogEntry) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(LogCommand::Access(entry));
        }
    }

    pub fn log_error(&self, entry: ErrorLogEntry) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(LogCommand::Error(entry));
        }
    }
}

impl Drop for AppLogger {
    fn drop(&mut self) {
        let _ = self.sender.take();
        if let Some(worker) = self.worker.lock().take() {
            let _ = worker.join();
        }
    }
}

pub fn classify_io_error(err: &io::Error) -> &'static str {
    match err.kind() {
        io::ErrorKind::ConnectionRefused => "target_refused",
        io::ErrorKind::TimedOut => "connect_timeout",
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => "protocol_error",
        io::ErrorKind::ConnectionReset
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::BrokenPipe => "connection_closed",
        _ => "io_error",
    }
}

#[derive(Debug)]
enum LogCommand {
    Access(AccessLogEntry),
    Error(ErrorLogEntry),
}

#[derive(Debug)]
struct WriterSet {
    access: RollingFileWriter,
    error: RollingFileWriter,
}

impl WriterSet {
    fn new(log_dir: PathBuf, config: LogConfig) -> io::Result<Self> {
        Ok(Self {
            access: RollingFileWriter::new(log_dir.clone(), ACCESS_LOG_NAME, config)?,
            error: RollingFileWriter::new(log_dir, ERROR_LOG_NAME, config)?,
        })
    }

    fn write_access(&mut self, entry: AccessLogEntry) -> io::Result<()> {
        let line = serde_json::to_string(&entry).map_err(io::Error::other)?;
        self.access.write_line(&line, Utc::now())
    }

    fn write_error(&mut self, entry: ErrorLogEntry) -> io::Result<()> {
        let line = serde_json::to_string(&entry).map_err(io::Error::other)?;
        self.error.write_line(&line, Utc::now())
    }
}

#[derive(Debug)]
struct RollingFileWriter {
    dir: PathBuf,
    base_name: String,
    config: LogConfig,
    writer: BufWriter<File>,
    current_size: u64,
    current_day: NaiveDate,
}

impl RollingFileWriter {
    fn new(dir: PathBuf, base_name: impl Into<String>, config: LogConfig) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let base_name = base_name.into();
        let file_path = dir.join(&base_name);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
        let current_size = file.metadata()?.len();
        Ok(Self {
            dir,
            base_name,
            config,
            writer: BufWriter::new(file),
            current_size,
            current_day: Utc::now().date_naive(),
        })
    }

    fn write_line(&mut self, line: &str, now: DateTime<Utc>) -> io::Result<()> {
        let entry_len = line.len() as u64 + 1;
        if self.should_rotate(now, entry_len) {
            self.rotate(now)?;
        }
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        self.current_size = self.current_size.saturating_add(entry_len);
        self.cleanup_expired_logs(SystemTime::now())?;
        Ok(())
    }

    fn should_rotate(&self, now: DateTime<Utc>, next_entry_len: u64) -> bool {
        self.current_day != now.date_naive()
            || self.current_size.saturating_add(next_entry_len) > self.config.rotation_size_bytes
    }

    fn rotate(&mut self, now: DateTime<Utc>) -> io::Result<()> {
        self.writer.flush()?;

        let oldest = self
            .dir
            .join(format!("{}.{}", self.base_name, self.config.max_rotations));
        if oldest.exists() {
            let _ = fs::remove_file(&oldest);
        }

        if self.config.max_rotations > 1 {
            for idx in (1..self.config.max_rotations).rev() {
                let src = self.dir.join(format!("{}.{}", self.base_name, idx));
                let dst = self.dir.join(format!("{}.{}", self.base_name, idx + 1));
                if src.exists() {
                    let _ = fs::rename(&src, &dst);
                }
            }
        }

        let current = self.dir.join(&self.base_name);
        if current.exists() {
            let _ = fs::rename(&current, self.dir.join(format!("{}.1", self.base_name)));
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&current)?;
        self.writer = BufWriter::new(file);
        self.current_size = 0;
        self.current_day = now.date_naive();
        self.cleanup_expired_logs(SystemTime::now())?;
        Ok(())
    }

    fn cleanup_expired_logs(&self, now: SystemTime) -> io::Result<()> {
        if self.config.retention_days == 0 {
            return Ok(());
        }
        let retention =
            Duration::from_secs(self.config.retention_days.saturating_mul(24 * 60 * 60));
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.starts_with(&self.base_name) || name == self.base_name {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified_at) = metadata.modified() else {
                continue;
            };
            let Ok(age) = now.duration_since(modified_at) else {
                continue;
            };
            if age > retention {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
}

fn iso_timestamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::{
        AccessLogEntry, AppLogger, ErrorLogEntry, LogConfig, RollingFileWriter, ACCESS_LOG_NAME,
    };

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        env::temp_dir().join(format!("wsl-bridge-{name}-{now}"))
    }

    #[test]
    fn rolling_writer_rotates_by_size() {
        let dir = temp_dir("log-rotate");
        fs::create_dir_all(&dir).expect("create dir");
        let mut writer = RollingFileWriter::new(
            dir.clone(),
            ACCESS_LOG_NAME,
            LogConfig {
                rotation_size_bytes: 24,
                max_rotations: 3,
                retention_days: 7,
            },
        )
        .expect("create writer");
        let now = Utc.with_ymd_and_hms(2026, 4, 19, 0, 0, 0).unwrap();

        writer.write_line(r#"{"a":1}"#, now).expect("write 1");
        writer.write_line(r#"{"b":2}"#, now).expect("write 2");
        writer.write_line(r#"{"c":3}"#, now).expect("write 3");

        assert!(dir.join("access.log").exists());
        assert!(dir.join("access.log.1").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rolling_writer_cleans_expired_files() {
        let dir = temp_dir("log-retention");
        fs::create_dir_all(&dir).expect("create dir");
        let expired = dir.join("access.log.2");
        fs::write(&expired, b"old").expect("write expired");

        let writer = RollingFileWriter::new(
            dir.clone(),
            ACCESS_LOG_NAME,
            LogConfig {
                rotation_size_bytes: 1024,
                max_rotations: 3,
                retention_days: 7,
            },
        )
        .expect("create writer");
        writer
            .cleanup_expired_logs(SystemTime::now() + Duration::from_secs(9 * 24 * 60 * 60))
            .expect("cleanup");

        assert!(!expired.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn app_logger_writes_json_lines() {
        let dir = temp_dir("app-logger");
        let logger = AppLogger::new_with_config(
            &dir,
            LogConfig {
                rotation_size_bytes: 1024,
                max_rotations: 3,
                retention_days: 7,
            },
        )
        .expect("create logger");

        logger.log_access(AccessLogEntry::success(
            "rule-a",
            "127.0.0.1:1234",
            "tcp",
            "FORWARD",
            "127.0.0.1:8080",
            12,
            34,
            56,
        ));
        logger.log_error(
            ErrorLogEntry::new("target_refused", "connect failed")
                .with_rule_id("rule-a")
                .with_client("127.0.0.1:1234")
                .with_target("127.0.0.1:8080")
                .with_detail(json!({ "phase": "connect" })),
        );
        drop(logger);

        let access = fs::read_to_string(dir.join("access.log")).expect("read access");
        let error = fs::read_to_string(dir.join("error.log")).expect("read error");
        assert!(access.contains("\"rule_id\":\"rule-a\""));
        assert!(error.contains("\"error_type\":\"target_refused\""));
        let _ = fs::remove_dir_all(dir);
    }
}
