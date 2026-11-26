use std::env;
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::write::GzEncoder;
use flate2::Compression;

const FRONTEND_CONFIG_FILES: &[&str] = &[
    "frontend/index.html",
    "frontend/package.json",
    "frontend/bun.lock",
    "frontend/vite.config.ts",
    "frontend/tsconfig.json",
    "frontend/postcss.config.js",
];

fn fnv1a_64(chunks: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for chunk in chunks {
        for &byte in *chunk {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_files_recursive(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn collect_frontend_inputs() -> Vec<PathBuf> {
    let mut inputs = Vec::new();
    for rel in FRONTEND_CONFIG_FILES {
        let p = PathBuf::from(rel);
        if p.is_file() {
            inputs.push(p);
        }
    }
    collect_files_recursive(Path::new("frontend/src"), &mut inputs);
    inputs.sort();
    inputs
}

fn compute_frontend_hash(inputs: &[PathBuf]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for path in inputs {
        let path_str = path.to_string_lossy();
        for byte in path_str.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
        if let Ok(bytes) = fs::read(path) {
            for byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

fn run_bun_build(html_path: &Path) -> Result<(), String> {
    if !Path::new("frontend/node_modules").exists() {
        println!("cargo:warning=frontend/node_modules missing, running `bun install`...");
        let install_status = Command::new("bun")
            .args(["install"])
            .current_dir("frontend")
            .status()
            .map_err(|e| format!("failed to spawn `bun install`: {e}"))?;
        if !install_status.success() {
            return Err(format!("`bun install` exited with {install_status}"));
        }
    }

    println!("cargo:warning=building frontend dashboard with `bun run build`...");
    let build_status = Command::new("bun")
        .args(["run", "build"])
        .current_dir("frontend")
        .status()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                "NOT_FOUND".to_string()
            } else {
                format!("failed to spawn `bun run build`: {e}")
            }
        })?;

    if !build_status.success() || !html_path.exists() {
        return Err(format!(
            "`bun run build` failed (status: {build_status}) or did not produce {}",
            html_path.display()
        ));
    }
    Ok(())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/dashboard.html");

    let inputs = collect_frontend_inputs();
    for path in &inputs {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR environment variable not set");
    let dest_path = PathBuf::from(&out_dir).join("dashboard.html.gz");
    let stamp_path = PathBuf::from(&out_dir).join("dashboard.stamp");
    let html_path = Path::new("assets/dashboard.html");

    let frontend_hash = compute_frontend_hash(&inputs);
    let current_html_bytes = fs::read(html_path).ok();
    let current_stamp = current_html_bytes
        .as_deref()
        .map(|bytes| format!("{frontend_hash:016x}:{:016x}", fnv1a_64(&[bytes])));
    let saved_stamp = fs::read_to_string(&stamp_path).ok();

    let needs_rebuild = match (&current_stamp, &saved_stamp) {
        (Some(cur), Some(saved)) => cur != saved.trim(),
        _ => true,
    };

    if needs_rebuild {
        match run_bun_build(html_path) {
            Ok(()) => {
                let rebuilt_bytes =
                    fs::read(html_path).expect("Failed to read rebuilt assets/dashboard.html");
                let new_stamp =
                    format!("{frontend_hash:016x}:{:016x}", fnv1a_64(&[&rebuilt_bytes]));
                let _ = fs::write(&stamp_path, new_stamp);
            }
            Err(err) if err == "NOT_FOUND" && html_path.exists() => {
                println!(
                    "cargo:warning=bun not found in PATH; using existing {} without rebuilding",
                    html_path.display()
                );
            }
            Err(err) => {
                panic!(
                    "Failed to build frontend dashboard ({err}). Run 'cargo fe-build' manually or ensure bun is in PATH."
                );
            }
        }
    }

    let html_bytes = fs::read(html_path).expect("Failed to read assets/dashboard.html");
    let output_file =
        File::create(&dest_path).expect("Failed to create destination dashboard.html.gz");
    let mut encoder = GzEncoder::new(output_file, Compression::best());
    encoder
        .write_all(&html_bytes)
        .expect("Failed to compress dashboard.html");
    encoder.finish().expect("Failed to finish gzip compression");
}
