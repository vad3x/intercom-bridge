use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use flate2::write::GzEncoder;
use flate2::Compression;

fn main() {
    println!("cargo:rerun-if-changed=assets/dashboard.html");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR environment variable not set");
    let dest_path = PathBuf::from(out_dir).join("dashboard.html.gz");

    let html_bytes = fs::read("assets/dashboard.html")
        .expect("Failed to read assets/dashboard.html");

    let output_file = File::create(&dest_path)
        .expect("Failed to create destination dashboard.html.gz");
    let mut encoder = GzEncoder::new(output_file, Compression::best());
    encoder
        .write_all(&html_bytes)
        .expect("Failed to compress dashboard.html");
    encoder
        .finish()
        .expect("Failed to finish gzip compression");
}
