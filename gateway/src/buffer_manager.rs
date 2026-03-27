use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use sensor_sim::accelerometer::AccelReading;
use sensor_sim::force_sensor::ForceReading;
use sensor_sim::thermometer::ThermoReading;
use sensor_sim::traits::Sensor;

// Define our own Reading enum to handle different sensor types
#[derive(Debug, Clone)]
pub enum UnifiedReading {
    Thermometer(ThermoReading),
    Accelerometer(AccelReading),
    ForceSensor(ForceReading),
}

// ============================================
// BUFFER STATS STRUCT
// ============================================

#[derive(Debug, Clone)]
pub struct BufferStats {
    pub total_readings: u64,
    pub current_buffer_size: usize,
    pub max_buffer_size: usize,
    pub total_reads: u64,
    pub total_writes: u64,
    pub overflow_warnings: u64,
}

impl Default for BufferStats {
    fn default() -> Self {
        Self {
            total_readings: 0,
            current_buffer_size: 0,
            max_buffer_size: 0,
            total_reads: 0,
            total_writes: 0,
            overflow_warnings: 0,
        }
    }
}

// ============================================
// SENSOR READING WITH METADATA
// ============================================

#[derive(Debug, Clone)]
pub struct TimestampedReading {
    pub reading: UnifiedReading,
    pub sensor_id: String,
    pub timestamp: Instant,
    pub sequence: u64,
}

// ============================================
// INTERNAL BUFFER STRUCT
// ============================================

struct InternalBuffer {
    queue: VecDeque<TimestampedReading>,
    capacity: usize,
    stats: BufferStats,
    next_sequence: u64,
}

impl InternalBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
            stats: BufferStats::default(),
            next_sequence: 0,
        }
    }

    fn push(&mut self, sensor_id: String, reading: UnifiedReading) -> bool {
        let timestamped = TimestampedReading {
            reading,
            sensor_id,
            timestamp: Instant::now(),
            sequence: self.next_sequence,
        };
        self.next_sequence += 1;

        let success = if self.queue.len() < self.capacity {
            self.queue.push_back(timestamped);
            true
        } else {
            self.stats.overflow_warnings += 1;
            false
        };

        self.stats.total_writes += 1;
        self.stats.current_buffer_size = self.queue.len();
        if self.queue.len() > self.stats.max_buffer_size {
            self.stats.max_buffer_size = self.queue.len();
        }

        success
    }

    fn pop(&mut self) -> Option<TimestampedReading> {
        let result = self.queue.pop_front();
        if result.is_some() {
            self.stats.total_reads += 1;
            self.stats.current_buffer_size = self.queue.len();
        }
        result
    }

    fn get_stats(&self) -> BufferStats {
        let mut stats = self.stats.clone();
        stats.current_buffer_size = self.queue.len();
        stats
    }
}

// ============================================
// SENSOR BUFFER MANAGER
// ============================================

pub struct SensorBufferManager {
    buffer: Arc<Mutex<InternalBuffer>>,
    condvar: Arc<Condvar>,
    running: Arc<AtomicBool>,
    reader_handles: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
}

// Helper trait to convert sensor readings to UnifiedReading
trait IntoUnifiedReading {
    fn into_unified(self) -> UnifiedReading;
}

impl IntoUnifiedReading for ThermoReading {
    fn into_unified(self) -> UnifiedReading {
        UnifiedReading::Thermometer(self)
    }
}

impl IntoUnifiedReading for AccelReading {
    fn into_unified(self) -> UnifiedReading {
        UnifiedReading::Accelerometer(self)
    }
}

impl IntoUnifiedReading for ForceReading {
    fn into_unified(self) -> UnifiedReading {
        UnifiedReading::ForceSensor(self)
    }
}

impl SensorBufferManager {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Buffer capacity must be positive");

        Self {
            buffer: Arc::new(Mutex::new(InternalBuffer::new(capacity))),
            condvar: Arc::new(Condvar::new()),
            running: Arc::new(AtomicBool::new(true)),
            reader_handles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // Register a thermometer sensor
    pub fn register_thermometer(
        &mut self,
        mut sensor: sensor_sim::thermometer::Thermometer,
    ) -> Result<(), String> {
        let sensor_id = sensor.id();
        let buffer = Arc::clone(&self.buffer);
        let condvar = Arc::clone(&self.condvar);
        let running = Arc::clone(&self.running);

        sensor.start();

        // Clone sensor_id for use in closure
        let sensor_id_clone = sensor_id.clone();
        let handle = thread::spawn(move || {
            let poll_interval = Duration::from_micros(100);
            let mut last_warning = Instant::now();

            while running.load(Ordering::Relaxed) {
                let available = sensor.available();

                if available > 100 && last_warning.elapsed() > Duration::from_secs(1) {
                    eprintln!(
                        "[WARNING] Sensor {} buffer has {} unread readings!",
                        sensor_id_clone, available
                    );
                    last_warning = Instant::now();
                }

                let mut readings_read = 0;
                while let Some(reading) = sensor.read() {
                    let mut buffer_guard = buffer.lock().unwrap();
                    let success =
                        buffer_guard.push(sensor_id_clone.clone(), reading.into_unified());
                    readings_read += 1;

                    if success {
                        condvar.notify_one();
                    } else {
                        eprintln!(
                            "[ERROR] Main buffer full! Reading from sensor {} dropped!",
                            sensor_id_clone
                        );
                    }
                }

                if readings_read > 0 {
                    let mut buffer_guard = buffer.lock().unwrap();
                    buffer_guard.stats.total_readings += readings_read as u64;
                }

                thread::sleep(poll_interval);
            }

            sensor.stop();
            eprintln!(
                "[INFO] Reader thread for sensor {} stopped",
                sensor_id_clone
            );
        });

        self.reader_handles.lock().unwrap().push(handle);
        eprintln!("[INFO] Registered thermometer sensor: {}", sensor_id);

        Ok(())
    }

    // Register an accelerometer sensor
    pub fn register_accelerometer(
        &mut self,
        mut sensor: sensor_sim::accelerometer::Accelerometer,
    ) -> Result<(), String> {
        let sensor_id = sensor.id();
        let buffer = Arc::clone(&self.buffer);
        let condvar = Arc::clone(&self.condvar);
        let running = Arc::clone(&self.running);

        sensor.start();

        // Clone sensor_id for use in closure
        let sensor_id_clone = sensor_id.clone();
        let handle = thread::spawn(move || {
            let poll_interval = Duration::from_micros(100);
            let mut last_warning = Instant::now();

            while running.load(Ordering::Relaxed) {
                let available = sensor.available();

                if available > 100 && last_warning.elapsed() > Duration::from_secs(1) {
                    eprintln!(
                        "[WARNING] Sensor {} buffer has {} unread readings!",
                        sensor_id_clone, available
                    );
                    last_warning = Instant::now();
                }

                let mut readings_read = 0;
                while let Some(reading) = sensor.read() {
                    let mut buffer_guard = buffer.lock().unwrap();
                    let success =
                        buffer_guard.push(sensor_id_clone.clone(), reading.into_unified());
                    readings_read += 1;

                    if success {
                        condvar.notify_one();
                    } else {
                        eprintln!(
                            "[ERROR] Main buffer full! Reading from sensor {} dropped!",
                            sensor_id_clone
                        );
                    }
                }

                if readings_read > 0 {
                    let mut buffer_guard = buffer.lock().unwrap();
                    buffer_guard.stats.total_readings += readings_read as u64;
                }

                thread::sleep(poll_interval);
            }

            sensor.stop();
            eprintln!(
                "[INFO] Reader thread for sensor {} stopped",
                sensor_id_clone
            );
        });

        self.reader_handles.lock().unwrap().push(handle);
        eprintln!("[INFO] Registered accelerometer sensor: {}", sensor_id);

        Ok(())
    }

    // Register a force sensor
    pub fn register_force_sensor(
        &mut self,
        mut sensor: sensor_sim::force_sensor::ForceSensor,
    ) -> Result<(), String> {
        let sensor_id = sensor.id();
        let buffer = Arc::clone(&self.buffer);
        let condvar = Arc::clone(&self.condvar);
        let running = Arc::clone(&self.running);

        sensor.start();

        // Clone sensor_id for use in closure
        let sensor_id_clone = sensor_id.clone();
        let handle = thread::spawn(move || {
            let poll_interval = Duration::from_micros(100);
            let mut last_warning = Instant::now();

            while running.load(Ordering::Relaxed) {
                let available = sensor.available();

                if available > 100 && last_warning.elapsed() > Duration::from_secs(1) {
                    eprintln!(
                        "[WARNING] Sensor {} buffer has {} unread readings!",
                        sensor_id_clone, available
                    );
                    last_warning = Instant::now();
                }

                let mut readings_read = 0;
                while let Some(reading) = sensor.read() {
                    let mut buffer_guard = buffer.lock().unwrap();
                    let success =
                        buffer_guard.push(sensor_id_clone.clone(), reading.into_unified());
                    readings_read += 1;

                    if success {
                        condvar.notify_one();
                    } else {
                        eprintln!(
                            "[ERROR] Main buffer full! Reading from sensor {} dropped!",
                            sensor_id_clone
                        );
                    }
                }

                if readings_read > 0 {
                    let mut buffer_guard = buffer.lock().unwrap();
                    buffer_guard.stats.total_readings += readings_read as u64;
                }

                thread::sleep(poll_interval);
            }

            sensor.stop();
            eprintln!(
                "[INFO] Reader thread for sensor {} stopped",
                sensor_id_clone
            );
        });

        self.reader_handles.lock().unwrap().push(handle);
        eprintln!("[INFO] Registered force sensor: {}", sensor_id);

        Ok(())
    }

    pub fn pop(&self) -> Option<TimestampedReading> {
        let mut buffer_guard = self.buffer.lock().unwrap();

        loop {
            if let Some(reading) = buffer_guard.pop() {
                return Some(reading);
            }

            buffer_guard = self.condvar.wait(buffer_guard).unwrap();

            if !self.running.load(Ordering::Relaxed) {
                return None;
            }
        }
    }

    pub fn pop_timeout(&self, timeout: Duration) -> Option<TimestampedReading> {
        let mut buffer_guard = self.buffer.lock().unwrap();

        loop {
            if let Some(reading) = buffer_guard.pop() {
                return Some(reading);
            }

            let (new_guard, result) = self.condvar.wait_timeout(buffer_guard, timeout).unwrap();
            buffer_guard = new_guard;

            if result.timed_out() {
                return None;
            }

            if !self.running.load(Ordering::Relaxed) {
                return None;
            }
        }
    }

    pub fn get_stats(&self) -> BufferStats {
        self.buffer.lock().unwrap().get_stats()
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer.lock().unwrap().stats.current_buffer_size
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.lock().unwrap().queue.is_empty()
    }

    pub fn shutdown(&mut self) {
        eprintln!("[INFO] Shutting down SensorBufferManager...");

        self.running.store(false, Ordering::Relaxed);
        self.condvar.notify_all();

        let mut handles = self.reader_handles.lock().unwrap();
        for handle in handles.drain(..) {
            let _ = handle.join();
        }

        eprintln!("[INFO] SensorBufferManager shutdown complete");
    }

    pub fn final_stats(&self) -> BufferStats {
        self.get_stats()
    }
}

impl Drop for SensorBufferManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.condvar.notify_all();
    }
}
