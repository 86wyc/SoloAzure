mod buffer_manager;
mod aggregation_engine;
mod storage;
mod webserver;   // new

use buffer_manager::SensorBufferManager;
use aggregation_engine::AggregationEngine;
use storage::DataStorage;
use webserver::WebServer;
use sensor_sim::{
    accelerometer::Accelerometer, force_sensor::ForceSensor, thermometer::Thermometer,
    traits::Sensor,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    println!("=== Sensor Data Aggregation Platform ===");
    println!("Components 2, 3, 4: Aggregation + Storage + Web Server\n");

    // Create buffer manager
    let mut buffer_manager = SensorBufferManager::new(10000);

    // Register sensors (same as before)
    let thermo_1 = Thermometer::new("thermo-1".to_string(), 10);
    let thermo_2 = Thermometer::new("thermo-2".to_string(), 10);
    let accel_1 = Accelerometer::new("accel-1".to_string(), 20);
    let accel_2 = Accelerometer::new("accel-2".to_string(), 20);
    let force_1 = ForceSensor::new("force-1".to_string(), 15);

    buffer_manager.register_thermometer(thermo_1).unwrap();
    buffer_manager.register_thermometer(thermo_2).unwrap();
    buffer_manager.register_accelerometer(accel_1).unwrap();
    buffer_manager.register_accelerometer(accel_2).unwrap();
    buffer_manager.register_force_sensor(force_1).unwrap();

    println!("All 5 sensors registered. Reader threads running...\n");

    let buffer_arc = Arc::new(buffer_manager);

    // Create data storage
    let storage = Arc::new(DataStorage::new("aggregated_stats.csv", "anomalies.csv")?);

    // Start web server in a background thread
    let stats_path = "aggregated_stats.csv".to_string();
    let anomalies_path = "anomalies.csv".to_string();
    let server = WebServer::new("127.0.0.1:8080", &stats_path, &anomalies_path)?;
    thread::spawn(move || {
        server.start();
    });

    // Create and start aggregation engine
    let mut engine = AggregationEngine::new(Duration::from_secs(1), 2.0, storage);
    engine.start(buffer_arc).expect("Failed to start engine");

    println!("Aggregation engine running. Web server at http://127.0.0.1:8080");
    println!("Collecting data for 20 seconds...\n");

    // Let the engine collect data for 20 seconds
    thread::sleep(Duration::from_secs(20));

    // Shut down engine
    engine.shutdown();

    println!("\n=== Aggregation test completed. Web server continues to run. ===");
    println!("Press Ctrl+C to exit.");
    // Keep the main thread alive to allow web server to continue
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}