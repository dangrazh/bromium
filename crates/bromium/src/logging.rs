use log::{LevelFilter, Metadata, Record};
use pyo3::prelude::*;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;

struct LogFileState {
    path: PathBuf,
    writer: BufWriter<File>,
}

impl LogFileState {
    fn open(path: PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            writer: BufWriter::new(file),
        })
    }
}

static LOGGER: BromiumLogger = BromiumLogger;
static LOG_LEVEL: Mutex<LevelFilter> = Mutex::new(LevelFilter::Debug);
static LOG_FILE: Mutex<Option<LogFileState>> = Mutex::new(None);
static LOG_TO_CONSOLE: Mutex<bool> = Mutex::new(false);
static LOG_TO_FILE: Mutex<bool> = Mutex::new(true);

struct BromiumLogger;

fn get_default_log_file() -> PathBuf {
    let mut log_path: PathBuf = env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir())
        .join(".bromium");

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
    log_path.join(format!("bromium_{}.log", timestamp))
}

impl log::Log for BromiumLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let level = LOG_LEVEL.lock().unwrap_or_else(|e| e.into_inner());
        metadata.level() <= *level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let log_message = format!(
                "{} | {}\t| {} | [{}, Line {}]",
                timestamp,
                record.level(),
                record.args(),
                record.module_path().unwrap_or("source module unknown"),
                record.line().unwrap_or(0)
            );

            if *LOG_TO_CONSOLE.lock().unwrap_or_else(|e| e.into_inner()) {
                println!("{}", log_message);
            }

            if *LOG_TO_FILE.lock().unwrap_or_else(|e| e.into_inner()) {
                let mut state = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());
                if state.is_none() {
                    *state = LogFileState::open(get_default_log_file()).ok();
                }
                if let Some(ref mut s) = *state {
                    let _ = writeln!(s.writer, "{}", log_message);
                    let _ = s.writer.flush();
                }
            }
        }
    }

    fn flush(&self) {
        if let Some(ref mut s) = *LOG_FILE.lock().unwrap_or_else(|e| e.into_inner()) {
            let _ = s.writer.flush();
        }
    }
}

#[pyclass]
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Off,
}

impl From<&str> for LogLevel {
    fn from(level_str: &str) -> Self {
        match level_str.to_lowercase().as_str() {
            "error" => LogLevel::Error,
            "warn" | "warning" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            "off" => LogLevel::Off,
            _ => LogLevel::Info,
        }
    }
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
            LogLevel::Off => LevelFilter::Off,
        }
    }
}

pub fn init_logger(
    log_dir: Option<PathBuf>,
    log_level: LevelFilter,
    enable_console: Option<bool>,
    enable_file: Option<bool>,
) {
    static INIT: std::sync::Once = std::sync::Once::new();

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
    let log_file_state = match LogFileState::open(log_file.clone()) {
        Ok(state) => Some(state),
        Err(e) => {
            eprintln!("Failed to open log file {}: {}", log_file.display(), e);
            None
        }
    };
    *LOG_FILE.lock().unwrap_or_else(|e| e.into_inner()) = log_file_state;

    *LOG_TO_CONSOLE.lock().unwrap_or_else(|e| e.into_inner()) = enable_console.unwrap_or(false);
    *LOG_TO_FILE.lock().unwrap_or_else(|e| e.into_inner()) = enable_file.unwrap_or(true);

    INIT.call_once(|| {
        if let Err(e) = log::set_logger(&LOGGER) {
            eprintln!(
                "Failed to install bromium logger (another logger may already be active): {e}"
            );
            return;
        }
        log::set_max_level(LevelFilter::Trace);
        log::info!(
            "Logger initialized. Default log file: {}",
            log_file.display()
        );
    });

    // Always apply the requested log level, even on repeated calls
    set_log_level_internal(log_level);
}

pub fn set_log_level_internal(level: LevelFilter) {
    let mut log_level = LOG_LEVEL.lock().unwrap_or_else(|e| e.into_inner());
    *log_level = level;
    log::set_max_level(level);
}

pub fn set_log_level(level: LogLevel) -> PyResult<()> {
    set_log_level_internal(level.into());
    log::info!("Log level set to: {:?}", level);
    Ok(())
}

pub fn get_log_level() -> PyResult<String> {
    let level = LOG_LEVEL.lock().unwrap_or_else(|e| e.into_inner());
    Ok(format!("{:?}", *level))
}

pub fn set_log_file(path: String) -> PyResult<()> {
    let path_buf = PathBuf::from(&path);

    if let Some(parent) = path_buf.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to create log directory: {}", e))
        })?;
    }

    {
        let mut state = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut s) = *state {
            let _ = s.writer.flush();
        }
        *state = Some(LogFileState::open(path_buf.clone()).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!(
                "Failed to open log file '{}': {}",
                path_buf.display(),
                e
            ))
        })?);
    }

    if *LOG_TO_FILE.lock().unwrap_or_else(|e| e.into_inner()) {
        log::info!("Log file changed to: {}", path_buf.display());
    }

    Ok(())
}

pub fn set_log_directory(dir_path: String) -> PyResult<()> {
    let dir_path_buf = PathBuf::from(&dir_path);

    if !dir_path_buf.exists() {
        std::fs::create_dir_all(&dir_path_buf).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to create log directory: {}", e))
        })?;
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let log_file = dir_path_buf.join(format!("bromium_{}.log", timestamp));

    {
        let mut state = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut s) = *state {
            let _ = s.writer.flush();
        }
        *state = Some(LogFileState::open(log_file.clone()).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!(
                "Failed to open log file '{}': {}",
                log_file.display(),
                e
            ))
        })?);
    }

    log::info!("Log directory changed to: {}", dir_path);
    log::info!("New log file: {}", log_file.display());

    Ok(())
}

pub fn get_log_file() -> PyResult<String> {
    let mut state = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());

    if state.is_none() {
        *state = LogFileState::open(get_default_log_file()).ok();
    }

    Ok(state
        .as_ref()
        .map(|s| s.path.to_string_lossy().to_string())
        .unwrap_or_default())
}

pub fn enable_console_logging(enable: bool) -> PyResult<()> {
    *LOG_TO_CONSOLE.lock().unwrap_or_else(|e| e.into_inner()) = enable;
    log::info!(
        "Console logging {}",
        if enable { "enabled" } else { "disabled" }
    );
    Ok(())
}

pub fn enable_file_logging(enable: bool) -> PyResult<()> {
    if enable {
        let mut state = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());
        if state.is_none() {
            let default_path = get_default_log_file();
            *state = Some(LogFileState::open(default_path.clone()).map_err(|e| {
                pyo3::exceptions::PyIOError::new_err(format!(
                    "Failed to open log file '{}': {}",
                    default_path.display(),
                    e
                ))
            })?);
        }
    }

    *LOG_TO_FILE.lock().unwrap_or_else(|e| e.into_inner()) = enable;
    log::info!(
        "File logging {}",
        if enable { "enabled" } else { "disabled" }
    );
    Ok(())
}

pub fn reset_log_file() -> PyResult<()> {
    let mut state = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(s) = state.take() {
        let path = s.path;
        drop(s.writer);
        File::create(&path).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to reset log file: {}", e))
        })?;
        *state = Some(LogFileState::open(path.clone()).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!(
                "Failed to reopen log file '{}': {}",
                path.display(),
                e
            ))
        })?);
    } else {
        return Err(pyo3::exceptions::PyValueError::new_err("No log file set"));
    }
    Ok(())
}

// ─── Module-level #[pyfunction] wrappers (R-02) ───────────────────────────────
// These expose the same functionality as Bromium static methods but as
// proper module-level functions: `bromium.init_logging(...)` etc.

/// Initialize the bromium logging system.
///
/// Parameters:
///     log_path (str | None): Directory for log files. Defaults to ~/.bromium.
///     log_level (str | None): One of "Off","Error","Warn","Info","Debug","Trace". Defaults to "Info".
///     enable_console (bool | None): Enable console output. Defaults to False.
///     enable_file (bool | None): Enable file output. Defaults to True.
#[pyfunction]
#[pyo3(name = "init_logging")]
pub fn py_init_logging(
    log_path: Option<&str>,
    log_level: Option<&str>,
    enable_console: Option<bool>,
    enable_file: Option<bool>,
) -> PyResult<()> {
    let log_dir = log_path.map(std::path::PathBuf::from);
    let log_level_parsed: LevelFilter = log_level
        .and_then(|s| s.parse().ok())
        .unwrap_or(LevelFilter::Info);
    init_logger(log_dir, log_level_parsed, enable_console, enable_file);
    log::info!("Bromium logging initialized.");
    Ok(())
}

/// Get the current bromium version string.
#[pyfunction]
#[pyo3(name = "get_version")]
pub fn py_get_version() -> PyResult<String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// Get the current log file path.
#[pyfunction]
#[pyo3(name = "get_log_file")]
pub fn py_get_log_file() -> PyResult<String> {
    get_log_file()
}

/// Set the full path for the log file. Creates parent directories if needed.
#[pyfunction]
#[pyo3(name = "set_log_file")]
pub fn py_set_log_file(log_file: &str) -> PyResult<()> {
    set_log_file(log_file.to_string())
}

/// Get the current log level as a string.
#[pyfunction]
#[pyo3(name = "get_log_level")]
pub fn py_get_log_level() -> PyResult<String> {
    get_log_level()
}

/// Set the log level. Accepts a LogLevel enum value or a string.
#[pyfunction]
#[pyo3(name = "set_log_level")]
pub fn py_set_log_level(log_level: &str) -> PyResult<()> {
    let level = LogLevel::from(log_level);
    set_log_level(level)
}

/// Set a custom directory for log files.
#[pyfunction]
#[pyo3(name = "set_log_directory")]
pub fn py_set_log_directory(log_directory: &str) -> PyResult<()> {
    set_log_directory(log_directory.to_string())
}

/// Enable or disable console logging.
#[pyfunction]
#[pyo3(name = "enable_console_logging")]
pub fn py_enable_console_logging(enable: bool) -> PyResult<()> {
    enable_console_logging(enable)
}

/// Enable or disable file logging.
#[pyfunction]
#[pyo3(name = "enable_file_logging")]
pub fn py_enable_file_logging(enable: bool) -> PyResult<()> {
    enable_file_logging(enable)
}

/// Clear all contents from the current log file.
#[pyfunction]
#[pyo3(name = "reset_log_file")]
pub fn py_reset_log_file() -> PyResult<()> {
    reset_log_file()
}
