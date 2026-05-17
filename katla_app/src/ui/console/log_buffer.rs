use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use log::{Level, LevelFilter, Log, Metadata, Record};

const DEFAULT_CAPACITY: usize = 16384;

pub(crate) struct LogEntry {
    pub timestamp: std::time::Instant,
    pub level: Level,
    pub message: String,
    pub target: String,
}

pub(crate) struct LogBuffer {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() == self.capacity {
            // Evict the oldest entry of the lowest priority level to
            // preserve higher-priority messages when verbose levels flood.
            let mut victim_idx = 0;
            let mut victim_level = self.entries[0].level;
            for (i, e) in self.entries.iter().enumerate() {
                if e.level > victim_level {
                    victim_idx = i;
                    victim_level = e.level;
                }
            }
            self.entries.remove(victim_idx);
        }
        self.entries.push_back(entry);
    }

    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

pub(crate) struct ConsoleLogger {
    buffer: Arc<Mutex<LogBuffer>>,
    level_filter: LevelFilter,
    secondary: Box<dyn Log>,
}

impl Log for ConsoleLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level_filter
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        self.secondary.log(record);

        let entry = LogEntry {
            timestamp: std::time::Instant::now(),
            level: record.level(),
            message: record.args().to_string(),
            target: record.target().to_string(),
        };

        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.push(entry);
        }
    }

    fn flush(&self) {}
}

pub(crate) struct ConsoleLoggerHandle {
    buffer: Arc<Mutex<LogBuffer>>,
    logger: ConsoleLogger,
}

impl ConsoleLoggerHandle {
    pub fn init(level_filter: LevelFilter, secondary: Box<dyn Log>) -> Self {
        let buffer = Arc::new(Mutex::new(LogBuffer::new()));
        let logger = ConsoleLogger {
            buffer: buffer.clone(),
            level_filter,
            secondary,
        };
        Self { buffer, logger }
    }

    pub fn buffer(&self) -> Arc<Mutex<LogBuffer>> {
        self.buffer.clone()
    }

    pub fn into_logger(self) -> Box<dyn Log> {
        Box::new(self.logger)
    }
}
