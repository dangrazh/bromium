#![allow(dead_code)]
use log::LevelFilter;
use log::{Metadata, Record};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub trait FromStrLevelFilter {
    fn from_str(level_str: &str) -> LevelFilter;
}

impl FromStrLevelFilter for LevelFilter {
    fn from_str(level_str: &str) -> Self {
        match level_str.to_lowercase().as_str() {
            "off" => LevelFilter::Off,
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => LevelFilter::Info,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstanceLogger {
    log_file: PathBuf,
    log_level: LevelFilter,
    enable_console: bool,
    enable_file: bool,
}
impl log::Log for InstanceLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.log_level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let log_message = format!(
                "{}: - bromium - {} - {}",
                timestamp,
                record.level(),
                record.args()
            );

            if self.enable_console {
                println!("{}", log_message);
            }

            if self.enable_file {
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log_file)
                {
                    let _ = writeln!(file, "{}", log_message);
                } else {
                    eprintln!("Failed to open log file: {}", self.log_file.display());
                }
            }
        }
    }

    fn flush(&self) {}
}

impl InstanceLogger {
    pub fn new(
        log_dir: Option<PathBuf>,
        log_level: LevelFilter,
        enable_console: Option<bool>,
        enable_file: Option<bool>,
    ) -> Self {
        let mut log_path: PathBuf = if let Some(dir) = log_dir {
            dir
        } else {
            env::var("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| env::temp_dir())
                .join(".bromium")
        };

        if !log_path.exists()
            && let Err(e) = std::fs::create_dir_all(&log_path)
        {
            eprintln!(
                "Failed to create default log directory {}: {}",
                log_path
                    .to_str()
                    .unwrap_or("failed to display log_dir PathBuf"),
                e
            );
            let fallback = env::temp_dir().join(".bromium");
            let _ = std::fs::create_dir_all(&fallback);
            log_path = fallback;
        }

        let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
        let log_file = log_path.join(format!("bromium_{}.log", timestamp));

        println!("Log file set to: {}", log_file.display());
        InstanceLogger {
            log_file,
            log_level,
            enable_console: enable_console.unwrap_or(false),
            enable_file: enable_file.unwrap_or(true),
        }
    }

    #[allow(dead_code)]
    pub fn set_log_level(&mut self, level: LevelFilter) {
        self.log_level = level;
    }

    #[allow(dead_code)]
    pub fn set_log_file(&mut self, path: PathBuf) {
        self.log_file = path;
    }

    pub fn get_log_file(&self) -> PathBuf {
        self.log_file.clone()
    }

    #[allow(dead_code)]
    pub fn enable_console(&mut self, enable: bool) {
        self.enable_console = enable;
    }

    #[allow(dead_code)]
    pub fn enable_file(&mut self, enable: bool) {
        self.enable_file = enable;
    }

    pub fn init_logger(
        log_dir: Option<PathBuf>,
        log_level: LevelFilter,
        enable_console: Option<bool>,
        enable_file: Option<bool>,
    ) -> InstanceLogger {
        static INIT: std::sync::Once = std::sync::Once::new();
        let logger_instance = InstanceLogger::new(log_dir, log_level, enable_console, enable_file);
        INIT.call_once(|| {
            log::set_boxed_logger(Box::new(logger_instance.clone()))
                .map(|()| log::set_max_level(log_level))
                .expect("Failed to initialize logger");

            log::info!("Logger initialized with log level: {}", log_level);
        });
        logger_instance
    }
}
