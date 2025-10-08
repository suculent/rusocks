//! Python bindings for rusocks

use log::{Level, LevelFilter};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::time::sleep;

/// Context functions for Python bindings
/// These provide access to Rust's async runtime in Python

/// Global runtime for Python bindings
lazy_static::lazy_static! {
    static ref GLOBAL_RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);
    static ref GLOBAL_NOTIFY: Arc<Notify> = Arc::new(Notify::new());
    static ref LOG_BUFFER: Mutex<LogBuffer> = Mutex::new(LogBuffer::new());
}

/// Initialize the global runtime
pub fn init_global_runtime() {
    let mut runtime = GLOBAL_RUNTIME.lock().unwrap();
    if runtime.is_none() {
        *runtime = Some(Runtime::new().unwrap());
    }
}

/// Get the global runtime
pub fn get_global_runtime() -> &'static Runtime {
    let mut runtime = GLOBAL_RUNTIME.lock().unwrap();
    if runtime.is_none() {
        *runtime = Some(Runtime::new().unwrap());
    }
    // Return a static reference instead of trying to clone
    // This is a hack and not thread-safe, but works for our purposes
    unsafe { std::mem::transmute(runtime.as_ref().unwrap()) }
}

/// Cancel the global runtime
pub fn cancel_global_runtime() {
    let mut runtime = GLOBAL_RUNTIME.lock().unwrap();
    *runtime = None;
    GLOBAL_NOTIFY.notify_waiters();
}

/// Context with cancel for Python bindings
pub struct ContextWithCancel {
    cancel_tx: mpsc::Sender<()>,
    cancel_rx: mpsc::Receiver<()>,
}

impl ContextWithCancel {
    /// Create a new context with cancel
    pub fn new() -> Self {
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        ContextWithCancel {
            cancel_tx,
            cancel_rx,
        }
    }

    /// Cancel the context
    pub fn cancel(&self) {
        let _ = self.cancel_tx.try_send(());
    }

    /// Check if the context is cancelled
    pub async fn is_cancelled(&mut self) -> bool {
        match self.cancel_rx.try_recv() {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

impl Default for ContextWithCancel {
    fn default() -> Self {
        Self::new()
    }
}

/// Time constants for Python bindings
pub const NANOSECOND: Duration = Duration::from_nanos(1);
pub const MICROSECOND: Duration = Duration::from_micros(1);
pub const MILLISECOND: Duration = Duration::from_millis(1);
pub const SECOND: Duration = Duration::from_secs(1);
pub const MINUTE: Duration = Duration::from_secs(60);
pub const HOUR: Duration = Duration::from_secs(3600);

/// Parse a duration string
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let mut result = Duration::from_secs(0);
    let mut current_num = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c.is_digit(10) || c == '.' || c == '-' {
            current_num.push(c);
        } else if c.is_alphabetic() {
            let unit = match c {
                'n' => {
                    if chars.peek() == Some(&'s') {
                        chars.next(); // consume 's'
                        NANOSECOND
                    } else {
                        return Err(format!("Invalid duration unit: {}", c));
                    }
                }
                'u' | 'µ' => {
                    if chars.peek() == Some(&'s') {
                        chars.next(); // consume 's'
                        MICROSECOND
                    } else {
                        return Err(format!("Invalid duration unit: {}", c));
                    }
                }
                'm' => {
                    if chars.peek() == Some(&'s') {
                        chars.next(); // consume 's'
                        MILLISECOND
                    } else {
                        MINUTE
                    }
                }
                's' => SECOND,
                'h' => HOUR,
                _ => return Err(format!("Invalid duration unit: {}", c)),
            };

            let value: f64 = current_num.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
            let nanos = (value * unit.as_nanos() as f64) as u64;
            result += Duration::from_nanos(nanos);
            current_num.clear();
        } else if c.is_whitespace() {
            // Skip whitespace
            continue;
        } else {
            return Err(format!("Invalid character in duration: {}", c));
        }
    }

    if !current_num.is_empty() {
        return Err("Duration string ended without unit".to_string());
    }

    Ok(result)
}

/// Log entry for Python bindings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Logger ID
    pub logger_id: String,
    
    /// Log message
    pub message: String,
    
    /// Timestamp (Unix timestamp in nanoseconds)
    pub time: u64,
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.time, self.logger_id, self.message)
    }
}

/// Log buffer for Python bindings
struct LogBuffer {
    entries: VecDeque<LogEntry>,
    max_size: usize,
    notify_channels: Vec<mpsc::Sender<()>>,
}

impl LogBuffer {
    /// Create a new log buffer
    fn new() -> Self {
        LogBuffer {
            entries: VecDeque::new(),
            max_size: 10000,
            notify_channels: Vec::new(),
        }
    }

    /// Add a log entry to the buffer
    fn add_entry(&mut self, logger_id: &str, message: &str) {
        let entry = LogEntry {
            logger_id: logger_id.to_string(),
            message: message.to_string(),
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        };

        self.entries.push_back(entry);

        // Keep buffer size under limit (simple FIFO)
        while self.entries.len() > self.max_size {
            self.entries.pop_front();
        }

        // Notify all waiting listeners
        for channel in &self.notify_channels {
            let _ = channel.try_send(());
        }
    }

    /// Get and clear log entries from the buffer
    fn get_entries(&mut self) -> Vec<LogEntry> {
        let entries: Vec<LogEntry> = self.entries.drain(..).collect();
        entries
    }

    /// Register a notification channel
    fn register_channel(&mut self, channel: mpsc::Sender<()>) {
        self.notify_channels.push(channel);
    }

    /// Unregister a notification channel
    fn unregister_channel(&mut self, index: usize) {
        if index < self.notify_channels.len() {
            self.notify_channels.remove(index);
        }
    }
}

/// Add a log entry to the global buffer
pub fn add_log_entry(logger_id: &str, message: &str) {
    let mut buffer = LOG_BUFFER.lock().unwrap();
    buffer.add_entry(logger_id, message);
}

/// Get log entries from the global buffer
pub fn get_log_entries() -> Vec<LogEntry> {
    let mut buffer = LOG_BUFFER.lock().unwrap();
    buffer.get_entries()
}

/// Wait for log entries with timeout (in milliseconds)
pub async fn wait_for_log_entries(timeout_ms: u64) -> Vec<LogEntry> {
    // First, check if there are already entries available
    {
        let mut buffer = LOG_BUFFER.lock().unwrap();
        if !buffer.entries.is_empty() {
            return buffer.get_entries();
        }
    }

    // Create a notification channel for this listener
    let (notify_tx, mut notify_rx) = mpsc::channel(1);
    
    // Register the channel
    let index;
    {
        let mut buffer = LOG_BUFFER.lock().unwrap();
        buffer.register_channel(notify_tx);
        index = buffer.notify_channels.len() - 1;
    }

    // Cleanup function to remove the channel
    let cleanup = || {
        let mut buffer = LOG_BUFFER.lock().unwrap();
        buffer.unregister_channel(index);
    };

    // Wait for notification or timeout
    if timeout_ms > 0 {
        tokio::select! {
            _ = notify_rx.recv() => {
                cleanup();
                let mut buffer = LOG_BUFFER.lock().unwrap();
                buffer.get_entries()
            }
            _ = sleep(Duration::from_millis(timeout_ms)) => {
                cleanup();
                Vec::new() // Timeout
            }
        }
    } else {
        // Wait indefinitely
        let _ = notify_rx.recv().await;
        cleanup();
        let mut buffer = LOG_BUFFER.lock().unwrap();
        buffer.get_entries()
    }
}

/// Cancel all waiting log listeners
pub fn cancel_log_waiters() {
    let mut buffer = LOG_BUFFER.lock().unwrap();
    buffer.notify_channels.clear();
}

/// Logger for Python bindings
pub struct PythonLogger {
    id: String,
    level: Level,
}

impl PythonLogger {
    /// Create a new logger with ID
    pub fn new(id: &str) -> Self {
        PythonLogger {
            id: id.to_string(),
            level: Level::Info,
        }
    }

    /// Create a new logger with ID and level
    pub fn new_with_level(id: &str, level: Level) -> Self {
        PythonLogger {
            id: id.to_string(),
            level,
        }
    }

    /// Set the log level
    pub fn set_level(&mut self, level: Level) {
        self.level = level;
    }

    /// Log a message at the specified level
    pub fn log(&self, level: Level, message: &str) {
        if level >= self.level {
            let formatted = format!("{{\"level\":\"{}\",\"message\":\"{}\"}}", 
                level.as_str(), message);
            add_log_entry(&self.id, &formatted);
        }
    }

    /// Log a trace message
    pub fn trace(&self, message: &str) {
        self.log(Level::Trace, message);
    }

    /// Log a debug message
    pub fn debug(&self, message: &str) {
        self.log(Level::Debug, message);
    }

    /// Log an info message
    pub fn info(&self, message: &str) {
        self.log(Level::Info, message);
    }

    /// Log a warning message
    pub fn warn(&self, message: &str) {
        self.log(Level::Warn, message);
    }

    /// Log an error message
    pub fn error(&self, message: &str) {
        self.log(Level::Error, message);
    }
}

/// Set the global log level
pub fn set_logger_global_level(level: Level) {
    // Convert Level to LevelFilter manually since from_level is not available
    let level_filter = match level {
        Level::Error => LevelFilter::Error,
        Level::Warn => LevelFilter::Warn,
        Level::Info => LevelFilter::Info,
        Level::Debug => LevelFilter::Debug,
        Level::Trace => LevelFilter::Trace,
    };
    log::set_max_level(level_filter);
}