use log::{LevelFilter, Metadata, Record}; // Level
use pyo3::prelude::*;
use std::sync::Mutex;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

static LOGGER: BromiumLogger = BromiumLogger;
static LOG_LEVEL: Mutex<LevelFilter> = Mutex::new(LevelFilter::Debug); // Default log level
static LOG_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
static LOG_TO_CONSOLE: Mutex<bool> = Mutex::new(false);
static LOG_TO_FILE: Mutex<bool> = Mutex::new(true);

// Predefined default log directory
#[cfg(target_os = "windows")]
const DEFAULT_LOG_DIR: &str = r"C:\bromium_logs";
#[cfg(not(target_os = "windows"))]
const DEFAULT_LOG_DIR: &str = "/tmp/bromium_logs";

struct BromiumLogger;

fn get_default_log_path() -> PathBuf {
    let log_dir = PathBuf::from(DEFAULT_LOG_DIR);
    
    // Create the directory if it doesn't exist
    if !log_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            // If we can't create the default directory, fall back to temp
            eprintln!("Failed to create default log directory {}: {}", DEFAULT_LOG_DIR, e);
            // Fallback to temp directory
            if let Ok(temp_dir) = std::env::var("TEMP").or_else(|_| std::env::var("TMP")) {
                let fallback = PathBuf::from(temp_dir).join("bromium_logs");
                let _ = std::fs::create_dir_all(&fallback);
                let timestamp = chrono::Local::now().format("%Y%m%d").to_string();
                return fallback.join(format!("bromium_{}.log", timestamp));
            }
        }
    }
    
    // Generate filename with timestamp
    let timestamp = chrono::Local::now().format("%Y%m%d").to_string();
    log_dir.join(format!("bromium_{}.log", timestamp))
}

impl log::Log for BromiumLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let level = LOG_LEVEL.lock().unwrap();
        metadata.level() <= *level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let log_message = format!("{}: - bromium - {} - {}", timestamp, record.level(), record.args());
            
            // Log to console if enabled
            if *LOG_TO_CONSOLE.lock().unwrap() {
                println!("{}", log_message);
            }
            
            // Log to file if enabled
            if *LOG_TO_FILE.lock().unwrap() {
                // Get log file path - use default if not set
                let log_path = {
                    let mut log_file = LOG_FILE.lock().unwrap();
                    if log_file.is_none() {
                        *log_file = Some(get_default_log_path());
                    }
                    log_file.clone().unwrap()
                };
                
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path) 
                {
                    let _ = writeln!(file, "{}", log_message);
                }
            }
        }
    }

    fn flush(&self) {}
}

#[pyclass]
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,  // Default level
    Trace,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
        }
    }
}

pub fn init_logger() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        log::set_logger(&LOGGER)
            .map(|()| log::set_max_level(LevelFilter::Trace))
            .expect("Failed to initialize logger");
        
        // Set default level to Debug
        set_log_level_internal(LevelFilter::Debug);
        
        // Initialize with default log file
        let default_path = get_default_log_path();
        *LOG_FILE.lock().unwrap() = Some(default_path.clone());
        
        log::info!("Logger initialized with default level: Debug");
        log::info!("Default log file: {}", default_path.display());
    });
}

pub fn set_log_level_internal(level: LevelFilter) {
    let mut log_level = LOG_LEVEL.lock().unwrap();
    *log_level = level;
    log::set_max_level(level);
}

#[pyfunction]
pub fn set_log_level(level: LogLevel) -> PyResult<()> {
    set_log_level_internal(level.into());
    log::info!("Log level set to: {:?}", level);
    Ok(())
}

#[pyfunction]
pub fn get_log_level() -> PyResult<String> {
    let level = LOG_LEVEL.lock().unwrap();
    Ok(format!("{:?}", *level))
}

#[pyfunction]
pub fn set_log_file(path: String) -> PyResult<()> {
    let path_buf = PathBuf::from(&path);
    
    // Create parent directories if they don't exist
    if let Some(parent) = path_buf.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(
                    format!("Failed to create log directory: {}", e)
                ))?;
        }
    }
    
    // Set the log file path
    *LOG_FILE.lock().unwrap() = Some(path_buf.clone());
    
    // If file logging is enabled, log the change
    if *LOG_TO_FILE.lock().unwrap() {
        log::info!("Log file changed to: {}", path_buf.display());
    }
    
    Ok(())
}

#[pyfunction]
pub fn set_log_directory(dir_path: String) -> PyResult<()> {
    let dir_path_buf = PathBuf::from(&dir_path);
    
    // Create directory if it doesn't exist
    if !dir_path_buf.exists() {
        std::fs::create_dir_all(&dir_path_buf)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(
                format!("Failed to create log directory: {}", e)
            ))?;
    }
    
    // Generate new log file path in the specified directory
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let log_file = dir_path_buf.join(format!("bromium_{}.log", timestamp));
    
    // Set the new log file path
    *LOG_FILE.lock().unwrap() = Some(log_file.clone());
    
    log::info!("Log directory changed to: {}", dir_path);
    log::info!("New log file: {}", log_file.display());
    
    Ok(())
}

#[pyfunction]
pub fn get_log_file() -> PyResult<String> {
    let mut log_file = LOG_FILE.lock().unwrap();
    
    // If no log file set, use default
    if log_file.is_none() {
        *log_file = Some(get_default_log_path());
    }
    
    Ok(log_file.as_ref().unwrap().to_string_lossy().to_string())
}

#[pyfunction]
pub fn get_default_log_directory() -> PyResult<String> {
    Ok(DEFAULT_LOG_DIR.to_string())
}

#[pyfunction]
pub fn enable_console_logging(enable: bool) -> PyResult<()> {
    *LOG_TO_CONSOLE.lock().unwrap() = enable;
    log::info!("Console logging {}", if enable { "enabled" } else { "disabled" });
    Ok(())
}

#[pyfunction]
pub fn enable_file_logging(enable: bool) -> PyResult<()> {
    // Ensure we have a log file path (use default if not set)
    if enable {
        let mut log_file = LOG_FILE.lock().unwrap();
        if log_file.is_none() {
            let default_path = get_default_log_path();
            log::info!("Using default log file: {}", default_path.display());
            *log_file = Some(default_path);
        }
    }
    
    *LOG_TO_FILE.lock().unwrap() = enable;
    log::info!("File logging {}", if enable { "enabled" } else { "disabled" });
    Ok(())
}

#[pyfunction]
pub fn reset_log_file() -> PyResult<()> {
    let log_file = LOG_FILE.lock().unwrap();
    
    if let Some(ref log_path) = *log_file {
        // Truncate the file (clear its contents)
        File::create(log_path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(
                format!("Failed to reset log file: {}", e)
            ))?;
        log::info!("Log file reset: {}", log_path.display());
    } else {
        return Err(pyo3::exceptions::PyValueError::new_err("No log file set"));
    }
    Ok(())
}