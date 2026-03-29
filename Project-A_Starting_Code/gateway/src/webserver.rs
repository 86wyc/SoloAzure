use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

/// A simple multi‑threaded web server that reads the CSV files and serves JSON.
pub struct WebServer {
    listener: TcpListener,
    stats_path: String,
    anomalies_path: String,
}

impl WebServer {
    /// Create a new server binding to the given address.
    pub fn new(addr: &str, stats_path: &str, anomalies_path: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            stats_path: stats_path.to_string(),
            anomalies_path: anomalies_path.to_string(),
        })
    }

    /// Start the server. This runs forever (until the program terminates).
    pub fn start(&self) {
        println!(
            "Web server listening on http://{}",
            self.listener.local_addr().unwrap()
        );

        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let stats_path = self.stats_path.clone();
                    let anomalies_path = self.anomalies_path.clone();
                    // Spawn a new thread for each connection (simple concurrency)
                    thread::spawn(move || {
                        handle_connection(stream, &stats_path, &anomalies_path);
                    });
                }
                Err(e) => eprintln!("Connection failed: {}", e),
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, stats_path: &str, anomalies_path: &str) {
    let mut buffer = [0; 1024];
    let n = match stream.read(&mut buffer) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to read request: {}", e);
            return;
        }
    };
    let request = String::from_utf8_lossy(&buffer[..n]);

    // Parse the first line to get method and path
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
        return;
    }
    let method = parts[0];
    let path = parts[1];

    if method != "GET" {
        let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n");
        return;
    }

    let response = match path {
        "/stats" => serve_all_stats(stats_path),
        "/latest" => serve_latest_stats(stats_path),
        path if path.starts_with("/sensor/") => {
            let sensor_id = &path[8..]; // after "/sensor/"
            serve_sensor_stats(stats_path, sensor_id)
        }
        _ => serve_index(),
    };

    let _ = stream.write_all(&response);
    let _ = stream.flush();
}

fn serve_index() -> Vec<u8> {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Sensor Data Aggregation Platform</title></head>
<body>
<h1>Sensor Data Aggregation Platform</h1>
<ul>
<li><a href="/stats">All Stats (JSON)</a></li>
<li><a href="/latest">Latest Frame Stats (JSON)</a></li>
<li><a href="/sensor/thermo-1">Stats for thermo-1</a></li>
<li><a href="/sensor/thermo-2">Stats for thermo-2</a></li>
<li><a href="/sensor/accel-1">Stats for accel-1</a></li>
<li><a href="/sensor/accel-2">Stats for accel-2</a></li>
<li><a href="/sensor/force-1">Stats for force-1</a></li>
</ul>
</body>
</html>"#;
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
        html.len(),
        html
    )
    .into_bytes()
}

fn serve_all_stats(path: &str) -> Vec<u8> {
    match read_stats_file(path) {
        Ok(stats) => {
            let json = serde_json::to_string(&stats).unwrap_or_else(|_| "[]".to_string());
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            )
            .into_bytes()
        }
        Err(_) => b"HTTP/1.1 500 Internal Server Error\r\n\r\n".to_vec(),
    }
}

fn serve_latest_stats(path: &str) -> Vec<u8> {
    match read_stats_file(path) {
        Ok(stats) => {
            let max_frame = stats.iter().map(|s| s.frame_id).max().unwrap_or(0);
            let latest: Vec<&StatEntry> =
                stats.iter().filter(|s| s.frame_id == max_frame).collect();
            let json = serde_json::to_string(&latest).unwrap_or_else(|_| "[]".to_string());
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            )
            .into_bytes()
        }
        Err(_) => b"HTTP/1.1 500 Internal Server Error\r\n\r\n".to_vec(),
    }
}

fn serve_sensor_stats(path: &str, sensor_id: &str) -> Vec<u8> {
    match read_stats_file(path) {
        Ok(stats) => {
            let filtered: Vec<&StatEntry> =
                stats.iter().filter(|s| s.sensor_id == sensor_id).collect();
            let json = serde_json::to_string(&filtered).unwrap_or_else(|_| "[]".to_string());
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            )
            .into_bytes()
        }
        Err(_) => b"HTTP/1.1 500 Internal Server Error\r\n\r\n".to_vec(),
    }
}

#[derive(serde::Serialize)]
struct StatEntry {
    frame_id: u64,
    sensor_id: String,
    count: u64,
    min: f64,
    max: f64,
    avg: f64,
    stddev: f64,
}

fn read_stats_file(path: &str) -> std::io::Result<Vec<StatEntry>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    // Skip header line
    for line in reader.lines().skip(1) {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 7 {
            let frame_id = parts[0].parse().unwrap_or(0);
            let sensor_id = parts[1].to_string();
            let count = parts[2].parse().unwrap_or(0);
            let min = parts[3].parse().unwrap_or(0.0);
            let max = parts[4].parse().unwrap_or(0.0);
            let avg = parts[5].parse().unwrap_or(0.0);
            let stddev = parts[6].parse().unwrap_or(0.0);
            entries.push(StatEntry {
                frame_id,
                sensor_id,
                count,
                min,
                max,
                avg,
                stddev,
            });
        }
    }
    Ok(entries)
}
