//! HTTP adapter for Intercom Bridge web dashboard and REST API.
//!
//! Provides a zero-runtime-allocation HTTP transport layer serving pre-compressed
//! static web dashboard assets and handling JSON telemetry and action requests.

#![no_std]

#[cfg(test)]
extern crate std;

pub mod router;
pub mod server;

pub use router::{format_status_json, parse_route, Route};
pub use server::{HttpServer, HttpServerConfig};

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
        decoder.read_to_string(&mut html).expect("Decompression failed");

        // Assert
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Intercom Bridge"));
        assert!(html.contains("btn-ring"));
        assert!(html.contains("btn-unlock"));
        assert!(html.contains("logs-pre"));
        assert!(DASHBOARD_HTML_GZ.len() < 5120, "Gzip asset exceeds 5 KiB limit");
    }
}
