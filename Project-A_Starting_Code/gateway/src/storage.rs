use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Mutex;

use crate::aggregation_engine::AggregatedFrame;

pub struct DataStorage {
    stats_writer: Mutex<BufWriter<File>>,
    anomalies_writer: Mutex<BufWriter<File>>,
}

impl DataStorage {
    pub fn new(stats_path: &str, anomalies_path: &str) -> std::io::Result<Self> {
        // Open stats file
        let stats_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(stats_path)?;
        let mut stats_writer = BufWriter::new(stats_file);

        // Write header if file is empty
        let metadata = std::fs::metadata(stats_path)?;
        if metadata.len() == 0 {
            writeln!(stats_writer, "frame_id,sensor_id,count,min,max,avg,stddev")?;
        }

        // Open anomalies file
        let anomalies_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(anomalies_path)?;
        let mut anomalies_writer = BufWriter::new(anomalies_file);

        let metadata = std::fs::metadata(anomalies_path)?;
        if metadata.len() == 0 {
            writeln!(anomalies_writer, "frame_id,sensor_id,severity,description")?;
        }

        Ok(Self {
            stats_writer: Mutex::new(stats_writer),
            anomalies_writer: Mutex::new(anomalies_writer),
        })
    }

    pub fn write_frame(&self, frame: &AggregatedFrame) -> std::io::Result<()> {
        // Write stats rows
        {
            let mut writer = self.stats_writer.lock().unwrap();
            for stat in &frame.stats {
                writeln!(
                    writer,
                    "{},{},{},{:.6},{:.6},{:.6},{:.6}",
                    frame.frame_id,
                    stat.sensor_id,
                    stat.count,
                    stat.min,
                    stat.max,
                    stat.average,
                    stat.stddev
                )?;
            }
        }

        // Write anomalies rows
        {
            let mut writer = self.anomalies_writer.lock().unwrap();
            for anomaly in &frame.anomalies {
                writeln!(
                    writer,
                    "{},{},{:.6},{}",
                    frame.frame_id, anomaly.sensor_id, anomaly.severity, anomaly.description
                )?;
            }
        }

        Ok(())
    }

    pub fn flush(&self) -> std::io::Result<()> {
        self.stats_writer.lock().unwrap().flush()?;
        self.anomalies_writer.lock().unwrap().flush()?;
        Ok(())
    }
}
