use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::buffer_manager::{SensorBufferManager, UnifiedReading};

// ============================================
// Data Structures for Aggregated Output
// ============================================

#[derive(Debug, Clone)]
pub struct SensorStats {
    pub sensor_id: String,
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub average: f64,
    pub stddev: f64,
}

#[derive(Debug, Clone)]
pub struct Anomaly {
    pub sensor_id: String,
    pub anomaly_type: String,
    pub severity: f64,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct AggregatedFrame {
    pub frame_id: u64,
    pub window_start: Instant,
    pub window_end: Instant,
    pub stats: Vec<SensorStats>,
    pub anomalies: Vec<Anomaly>,
}

// ============================================
// Aggregation Engine
// ============================================

pub struct AggregationEngine {
    window_duration: Duration,
    anomaly_threshold: f64,
    running: Arc<AtomicBool>,
    collector_handle: Option<thread::JoinHandle<()>>,
}

impl AggregationEngine {
    pub fn new(window_duration: Duration, anomaly_threshold: f64) -> Self {
        Self {
            window_duration,
            anomaly_threshold,
            running: Arc::new(AtomicBool::new(true)),
            collector_handle: None,
        }
    }

    pub fn start(&mut self, buffer: Arc<SensorBufferManager>) -> Result<(), String> {
        if self.collector_handle.is_some() {
            return Err("Engine already started".into());
        }

        let running = Arc::clone(&self.running);
        let window_duration = self.window_duration;
        let anomaly_threshold = self.anomaly_threshold;
        let buffer_clone = Arc::clone(&buffer);

        let handle = thread::spawn(move || {
            Self::collector_loop(running, buffer_clone, window_duration, anomaly_threshold);
        });

        self.collector_handle = Some(handle);
        Ok(())
    }

    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.collector_handle.take() {
            let _ = handle.join();
        }
    }

    fn collector_loop(
        running: Arc<AtomicBool>,
        buffer: Arc<SensorBufferManager>,
        window_duration: Duration,
        anomaly_threshold: f64,
    ) {
        let mut frame_id = 0;
        let mut current_window: HashMap<String, Vec<f64>> = HashMap::new();
        let mut window_start = Instant::now();

        println!(
            "[Aggregation] Collector started. Window duration: {:?}",
            window_duration
        );

        while running.load(Ordering::Relaxed) {
            match buffer.pop_timeout(Duration::from_millis(100)) {
                Some(reading) => {
                    let value = reading_to_value(&reading.reading);
                    if let Some(val) = value {
                        current_window
                            .entry(reading.sensor_id.clone())
                            .or_insert_with(Vec::new)
                            .push(val);
                    }
                }
                None => {}
            }

            if window_start.elapsed() >= window_duration {
                let frame = Self::compute_frame(
                    frame_id,
                    window_start,
                    window_start + window_duration,
                    &current_window,
                    anomaly_threshold,
                );
                println!("{}", frame.pretty_print());

                frame_id += 1;
                current_window.clear();
                window_start = Instant::now();
            }
        }

        println!("[Aggregation] Collector stopped.");
    }

    fn compute_frame(
        frame_id: u64,
        start: Instant,
        end: Instant,
        window_data: &HashMap<String, Vec<f64>>,
        anomaly_threshold: f64,
    ) -> AggregatedFrame {
        let mut stats = Vec::new();
        let mut anomalies = Vec::new();

        for (sensor_id, values) in window_data {
            if values.is_empty() {
                continue;
            }

            let count = values.len() as u64;
            let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let sum: f64 = values.iter().sum();
            let avg = sum / count as f64;

            let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / count as f64;
            let stddev = variance.sqrt();

            stats.push(SensorStats {
                sensor_id: sensor_id.clone(),
                count,
                min,
                max,
                average: avg,
                stddev,
            });

            if stddev > 1e-6 {
                for &val in values {
                    let deviation = (val - avg).abs() / stddev;
                    if deviation > anomaly_threshold {
                        anomalies.push(Anomaly {
                            sensor_id: sensor_id.clone(),
                            anomaly_type: "Outlier".into(),
                            severity: deviation,
                            description: format!(
                                "Value {:.2} is {:.1}σ from mean {:.2}",
                                val, deviation, avg
                            ),
                        });
                    }
                }
            }
        }

        AggregatedFrame {
            frame_id,
            window_start: start,
            window_end: end,
            stats,
            anomalies,
        }
    }
}

fn reading_to_value(reading: &UnifiedReading) -> Option<f64> {
    match reading {
        UnifiedReading::Thermometer(t) => Some(t.temperature_celsius.into()),
        UnifiedReading::Accelerometer(a) => {
            let mag = (a.acceleration_x * a.acceleration_x
                + a.acceleration_y * a.acceleration_y
                + a.acceleration_z * a.acceleration_z)
                .sqrt();
            Some(mag.into())
        }
        UnifiedReading::ForceSensor(f) => {
            let mag =
                (f.force_x * f.force_x + f.force_y * f.force_y + f.force_z * f.force_z).sqrt();
            Some(mag.into())
        }
    }
}

impl AggregatedFrame {
    fn pretty_print(&self) -> String {
        let duration = self.window_end - self.window_start;
        let mut output = format!(
            "\n=== Aggregated Frame {} (window: {:?}) ===\n",
            self.frame_id, duration
        );
        for stat in &self.stats {
            output.push_str(&format!(
                "  Sensor {}: count={}, min={:.2}, max={:.2}, avg={:.2}, stddev={:.2}\n",
                stat.sensor_id, stat.count, stat.min, stat.max, stat.average, stat.stddev
            ));
        }
        if !self.anomalies.is_empty() {
            output.push_str("  Anomalies:\n");
            for anom in &self.anomalies {
                output.push_str(&format!(
                    "    {}: {} (severity {:.1}) - {}\n",
                    anom.sensor_id, anom.anomaly_type, anom.severity, anom.description
                ));
            }
        }
        output
    }
}
