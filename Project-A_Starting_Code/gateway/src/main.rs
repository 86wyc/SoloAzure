mod aggregation_engine;
mod buffer_manager;
mod storage;

use aggregation_engine::AggregationEngine;
use buffer_manager::SensorBufferManager;
use sensor_sim::{
    accelerometer::Accelerometer, force_sensor::ForceSensor, thermometer::Thermometer,
    traits::Sensor,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use storage::DataStorage;

fn main() -> std::io::Result<()> {
    println!("=== Sensor Data Aggregation Platform ===");
    println!("Components 2 & 3: Aggregation Engine + Data Storage\n");

    // Create buffer manager
    let mut buffer_manager = SensorBufferManager::new(10000);

    // Register sensors
    let thermo_1 = Thermometer::new("thermo-1".to_string(), 10);
    let thermo_2 = Thermometer::new("thermo-2".to_string(), 10);
    let accel_1 = Accelerometer::new("accel-1".to_string(), 20);
    let accel_2 = Accelerometer::new("accel-2".to_string(), 20);
    let force_1 = ForceSensor::new("force-1".to_string(), 15);

    buffer_manager
        .register_thermometer(thermo_1)
        .expect("Failed to register thermo-1");
    buffer_manager
        .register_thermometer(thermo_2)
        .expect("Failed to register thermo-2");
    buffer_manager
        .register_accelerometer(accel_1)
        .expect("Failed to register accel-1");
    buffer_manager
        .register_accelerometer(accel_2)
        .expect("Failed to register accel-2");
    buffer_manager
        .register_force_sensor(force_1)
        .expect("Failed to register force-1");

    println!("All 5 sensors registered. Reader threads running...\n");

    let buffer_arc = Arc::new(buffer_manager);

    // Create data storage
    let storage = Arc::new(DataStorage::new("aggregated_stats.csv", "anomalies.csv")?);

    // Create and start aggregation engine
    let mut engine = AggregationEngine::new(Duration::from_secs(1), 2.0, storage);
    engine.start(buffer_arc).expect("Failed to start engine");

    println!("Aggregation engine running. Collecting data for 20 seconds...\n");

    // Let the engine collect data for 20 seconds
    thread::sleep(Duration::from_secs(20));

    // Shut down engine
    engine.shutdown();

    println!("\n=== Aggregation test completed ===");
    Ok(())
}
