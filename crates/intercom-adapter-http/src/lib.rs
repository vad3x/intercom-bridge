//! HTTP adapter for Intercom Bridge web dashboard and REST API.
//!
//! Provides a zero-runtime-allocation HTTP transport layer serving pre-compressed
//! static web dashboard assets and handling JSON telemetry and action requests.

#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod config;
pub mod router;
pub mod server;

pub use config::HttpServerConfig;
pub use router::{
    format_status_json, format_status_json_with_telemetry, parse_route, CpuCoreLoad, CpuSnapshot,
    IntercomStatusDto, MemoryRegion, MemorySnapshot, Route, MAX_CPU_CORES,
};
pub use server::HttpServer;

/// Pre-gzipped single-page dashboard application compiled into the binary.
pub const DASHBOARD_HTML_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.html.gz"));

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Read;

    #[test]
    fn test_dashboard_gzip_decompresses_to_valid_html() {
        // Arrange
        let mut decoder = GzDecoder::new(DASHBOARD_HTML_GZ);
        let mut html = std::string::String::new();

        // Act
        decoder
            .read_to_string(&mut html)
            .expect("Decompression failed");

        // Assert
        assert!(html.to_lowercase().contains("<!doctype html>"));
        assert!(html.contains("Intercom Bridge"));
        assert!(
            DASHBOARD_HTML_GZ.len() < 50 * 1024,
            "Gzip asset exceeds 50 KiB limit: {} bytes",
            DASHBOARD_HTML_GZ.len()
        );
    }
}
