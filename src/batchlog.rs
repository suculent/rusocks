//! Batch logging for rusocks

use log::Level;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

/// BatchLogger buffers log messages and flushes them periodically
pub struct BatchLogger {
    /// Buffer for log messages
    buffer: Arc<Mutex<VecDeque<String>>>,

    /// Maximum buffer size
    max_size: usize,

    /// Flush interval
    flush_interval: Duration,

    /// Last flush time
    last_flush: Arc<Mutex<Instant>>,

    /// Shutdown channel
    shutdown_tx: mpsc::Sender<()>,

    /// Shutdown receiver
    shutdown_rx: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
}

impl BatchLogger {
    /// Create a new BatchLogger
    pub fn new(max_size: usize, flush_interval: Duration) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        let logger = BatchLogger {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(max_size))),
            max_size,
            flush_interval,
            last_flush: Arc::new(Mutex::new(Instant::now())),
            shutdown_tx,
            shutdown_rx: Arc::new(Mutex::new(Some(shutdown_rx))),
        };

        // Start background flush task
        logger.start_flush_task();

        logger
    }

    /// Start the background flush task
    fn start_flush_task(&self) {
        let buffer = self.buffer.clone();
        let flush_interval = self.flush_interval;
        let last_flush = self.last_flush.clone();
        let _max_size = self.max_size;

        let mut shutdown_rx = self.shutdown_rx.lock().unwrap().take().unwrap();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = sleep(flush_interval) => {
                        // Check if we need to flush
                        let now = Instant::now();
                        let mut last = last_flush.lock().unwrap();
                        if now.duration_since(*last) >= flush_interval {
                            // Flush the buffer
                            let mut buf = buffer.lock().unwrap();
                            if !buf.is_empty() {
                                // Process the logs (in a real implementation, this would write to a file or send to a server)
                                buf.clear();
                            }
                            *last = now;
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        // Shutdown requested
                        break;
                    }
                }
            }

            // Final flush
            let mut buf = buffer.lock().unwrap();
            if !buf.is_empty() {
                // Process the logs
                buf.clear();
            }
        });
    }

    /// Log a message
    pub fn log(&self, level: Level, message: &str) {
        let mut buffer = self.buffer.lock().unwrap();

        // Format the message
        let formatted = format!("[{}] {}", level, message);

        // Add to buffer
        buffer.push_back(formatted);

        // Check if buffer is full
        if buffer.len() >= self.max_size {
            // Remove oldest entries
            while buffer.len() > self.max_size / 2 {
                buffer.pop_front();
            }
        }
    }

    /// Flush the buffer
    pub fn flush(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        if !buffer.is_empty() {
            // Process the logs
            buffer.clear();
        }

        // Update last flush time
        let mut last = self.last_flush.lock().unwrap();
        *last = Instant::now();
    }

    /// Close the logger
    pub async fn close(&self) {
        // Send shutdown signal
        let _ = self.shutdown_tx.send(()).await;

        // Final flush
        self.flush();
    }
}

impl Drop for BatchLogger {
    fn drop(&mut self) {
        // Final flush
        self.flush();
    }
}
