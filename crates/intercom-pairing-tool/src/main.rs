//! Intercom Bridge - Matter Commissioning QR & Pairing Code Tool
//!
//! Generates official Matter manual pairing codes and setup QR code
//! for a given device MAC address using `rs-matter` directly.

use std::fs;
use std::process::Command;

use rs_matter::pairing::qr::{no_optional_data, CommFlowType, Qr, QrPayload, QrTextType};
use rs_matter::pairing::DiscoveryCapabilities;
use rs_matter::BasicCommData;

const VID: u16 = 0xfff1;
const PID: u16 = 0x8001;

fn print_usage() {
    eprintln!(
        r#"Usage: cargo qr [OPTIONS]

Options:
  --mac <MAC>     6-byte hardware MAC address (e.g. 98:a3:16:a6:78:44 or 98A316A67844)
  --port <PORT>   Serial port of the connected ESP32-C6 (auto-detected if omitted)
  --json          Output results as JSON
  -h, --help      Print this help message
"#
    );
}

fn find_serial_port() -> Option<String> {
    let mut ports = Vec::new();
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name.starts_with("cu.usbmodem") || name.starts_with("cu.usbserial") {
                    ports.push(format!("/dev/{}", name));
                }
            }
        }
    }
    ports.sort();
    ports.into_iter().next()
}

fn query_mac_from_espflash(port: &str) -> Result<String, String> {
    eprintln!("Querying device MAC address on {}...", port);

    // Try "espflash" first, then "mise exec -- espflash"
    let output = Command::new("espflash")
        .args(["board-info", "--port", port])
        .output()
        .or_else(|_| {
            Command::new("mise")
                .args(["exec", "--", "espflash", "board-info", "--port", port])
                .output()
        })
        .map_err(|e| format!("Failed to execute espflash: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        if combined.contains("Resource busy") || combined.contains("Device or resource busy") {
            return Err(format!(
                "Serial port {} is busy. Ensure other monitors (e.g. monitor.py) are stopped.",
                port
            ));
        }
        return Err(format!("espflash board-info failed:\n{}", combined));
    }

    for line in combined.lines() {
        if line.contains("MAC address:") {
            if let Some(mac_part) = line.split("MAC address:").nth(1) {
                let trimmed = mac_part.trim();
                if !trimmed.is_empty() {
                    return Ok(trimmed.to_string());
                }
            }
        }
    }

    Err(format!("Could not find MAC address in espflash output:\n{}", combined))
}

fn parse_mac(s: &str) -> Result<[u8; 6], &'static str> {
    let clean: String = s
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();

    if clean.len() != 12 {
        return Err("MAC address must be 12 hex characters (e.g. AA:BB:CC:DD:EE:FF)");
    }

    let mut bytes = [0u8; 6];
    for i in 0..6 {
        bytes[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16)
            .map_err(|_| "Invalid hex in MAC address")?;
    }

    Ok(bytes)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut mac_str: Option<String> = None;
    let mut port_str: Option<String> = None;
    let mut json_mode = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mac" | "-m" => {
                mac_str = args.next();
            }
            "--port" | "-p" => {
                port_str = args.next();
            }
            "--json" => {
                json_mode = true;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other if !other.starts_with('-') && mac_str.is_none() => {
                mac_str = Some(other.to_string());
            }
            _ => {
                eprintln!("Unknown argument: {}", arg);
                print_usage();
                std::process::exit(1);
            }
        }
    }

    let mac_input = match mac_str {
        Some(m) => m,
        None => {
            let port = match port_str {
                Some(p) => p,
                None => match find_serial_port() {
                    Some(p) => p,
                    None => {
                        eprintln!("Error: No ESP32-C6 USB device detected.");
                        eprintln!("Connect your device via USB or specify a MAC address manually:");
                        eprintln!("    cargo qr --mac 98:a3:16:a6:78:44");
                        std::process::exit(1);
                    }
                },
            };

            match query_mac_from_espflash(&port) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    let mac_bytes = match parse_mac(&mac_input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let formatted_mac = format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac_bytes[0], mac_bytes[1], mac_bytes[2], mac_bytes[3], mac_bytes[4], mac_bytes[5]
    );

    let passcode = intercom_core::domain::commissioning::passcode_from_mac(&mac_bytes);
    let discriminator = intercom_core::domain::commissioning::discriminator_from_mac(&mac_bytes);
    let serial_buf = intercom_core::domain::commissioning::serial_no_from_mac(&mac_bytes);
    let serial_no = core::str::from_utf8(&serial_buf).unwrap_or("IC-UNKNOWN");

    let comm_data = BasicCommData {
        password: passcode.to_le_bytes().into(),
        discriminator,
    };

    let short_code = comm_data.compute_pretty_pairing_code();
    let long_code = comm_data.compute_pretty_pairing_code_long(VID, PID);

    let dev_det = rs_matter::dm::clusters::basic_info::BasicInfoConfig {
        vid: VID,
        pid: PID,
        serial_no,
        ..rs_matter::dm::clusters::basic_info::BasicInfoConfig::new()
    };

    let payload = QrPayload::new_from_basic_info(
        DiscoveryCapabilities::BLE,
        CommFlowType::Standard,
        comm_data,
        &dev_det,
        no_optional_data,
    );

    let mut qr_str_buf = [0u8; 128];
    let qr_text = match payload.as_str(&mut qr_str_buf) {
        Ok((text, _)) => text.to_string(),
        Err(e) => {
            eprintln!("Error generating QR text: {:?}", e);
            std::process::exit(1);
        }
    };

    if json_mode {
        println!("{{");
        println!("  \"mac\": \"{}\",", formatted_mac);
        println!("  \"serial_number\": \"{}\",", serial_no);
        println!("  \"vendor_id\": {},", VID);
        println!("  \"product_id\": {},", PID);
        println!("  \"discriminator\": {},", discriminator);
        println!("  \"passcode\": {},", passcode);
        println!("  \"manual_code\": \"{}\",", short_code);
        println!("  \"manual_code_long\": \"{}\",", long_code);
        println!("  \"qr_code\": \"{}\"", qr_text);
        println!("}}");
        return;
    }

    println!("================================================================================");
    println!("  Smart Intercom — Matter Commissioning Credentials");
    println!("================================================================================");
    println!();
    println!("  Device MAC:          {}", formatted_mac);
    println!("  Serial Number:       {}", serial_no);
    println!("  Vendor ID / PID:     0x{:04X} ({}) / 0x{:04X} ({})", VID, VID, PID, PID);
    println!("  Discriminator:       {} (0x{:03X})", discriminator, discriminator);
    println!("  Passcode (PIN):      {}", passcode);
    println!();
    println!("  Manual Pairing Code (Standard 11-digit):");
    println!("  ┌─────────────────┐");
    println!("  │  {:^13}  │", short_code);
    println!("  └─────────────────┘");
    println!();
    println!("  Manual Pairing Code (Long 21-digit with VID/PID):");
    println!("  ┌─────────────────────────────┐");
    println!("  │  {:^25}  │", long_code);
    println!("  └─────────────────────────────┘");
    println!();
    println!("  Matter Setup QR Code Payload:");
    println!("  {}", qr_text);
    println!();
    println!("  Scan with Apple Home / Google Home / SmartThings:");
    println!();

    // Compute and display QR matrix
    let mut tmp_buf = [0u8; 1024];
    let mut out_buf = [0u8; 1024];
    match Qr::compute(&qr_text, &mut tmp_buf, &mut out_buf) {
        Ok(qr) => {
            const BORDER_SIZE: u8 = 2;
            for y in qr.lines_range(QrTextType::Unicode, BORDER_SIZE) {
                let mut line_buf = [0u8; 512];
                if let Ok((line, _)) = qr.line_as_str(
                    QrTextType::Unicode,
                    BORDER_SIZE,
                    false,
                    false,
                    y,
                    &mut line_buf,
                ) {
                    println!("  {}", line);
                }
            }
        }
        Err(e) => {
            eprintln!("  (Failed to render QR matrix in terminal: {:?})", e);
        }
    }

    println!();
}
