mod buffer_manager;

use buffer_manager::SensorBufferManager;
use sensor_sim::{
    accelerometer::Accelerometer, force_sensor::ForceSensor, thermometer::Thermometer,
    traits::Sensor,
};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    println!("=== Sensor Data Aggregation Platform ===");
    println!("Component 1: Buffer Management - Zero Data Loss Test\n");

    let mut buffer_manager = SensorBufferManager::new(10000);

    println!("Creating and registering sensors...");

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

    let buffer_manager_arc = Arc::new(buffer_manager);
    let consumer_buffer = Arc::clone(&buffer_manager_arc);

    // Consumer thread that reads from buffer for a fixed duration (15 seconds)
    let consumer_handle = thread::spawn(move || {
        let mut consumed_count = 0;
        let start_time = Instant::now();
        let test_duration = Duration::from_secs(15);

        while start_time.elapsed() < test_duration {
            match consumer_buffer.pop_timeout(Duration::from_millis(100)) {
                Some(reading) => {
                    consumed_count += 1;
                    if consumed_count % 100 == 0 {
                        let stats = consumer_buffer.get_stats();
                        println!(
                            "[Consumer] Consumed: {} | Buffer size: {} | Max buffer size: {}",
                            consumed_count, stats.current_buffer_size, stats.max_buffer_size
                        );
                    }
                }
                None => {
                    // No reading, just continue
                }
            }
        }

        println!(
            "\n[Consumer] Total readings consumed in {} seconds: {}",
            test_duration.as_secs(),
            consumed_count
        );
        consumed_count
    });

    // Let the system run for 15 seconds total (we'll let consumer run the full duration)
    // Wait for consumer to finish
    let _consumed = consumer_handle.join().unwrap();

    // After consumer finishes, print final statistics
    let final_stats = buffer_manager_arc.get_stats();
    println!("\n=== Final Statistics ===");
    println!("Total readings stored: {}", final_stats.total_readings);
    println!("Total writes: {}", final_stats.total_writes);
    println!("Current buffer size: {}", final_stats.current_buffer_size);
    println!("Max buffer size reached: {}", final_stats.max_buffer_size);
    println!("Overflow warnings: {}", final_stats.overflow_warnings);

    if final_stats.overflow_warnings == 0 {
        println!("\n✅ SUCCESS: ZERO DATA LOSS ACHIEVED!");
        println!("   All sensor readings were successfully captured without overflow.");
    } else {
        println!(
            "\n❌ FAILURE: {} data loss events occurred!",
            final_stats.overflow_warnings
        );
        println!("   Check polling frequency and buffer size.");
    }

    // Calculate expected vs actual
    let expected_readings = (10 * 2) + (20 * 2) + 15; // readings per second
    let expected_total = expected_readings * 15; // 15 seconds
    println!("\n=== Performance Summary ===");
    println!("Expected readings (approx): ~{}", expected_total);
    println!("Actual readings stored: {}", final_stats.total_readings);
    println!(
        "Capture rate: {:.1}%",
        (final_stats.total_readings as f64 / expected_total as f64) * 100.0
    );

    // Shutdown the buffer manager (stops reader threads)
    // Note: we can't call shutdown because buffer_manager is inside Arc, but we can drop the Arc
    drop(buffer_manager_arc);
    println!("\n=== Component 1 Test Complete ===");
}
