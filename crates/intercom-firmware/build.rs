#[path = "src/platform/target_selection.rs"]
mod target_selection;

fn main() {
    validate_firmware_target();
    edf_build::emit_version_env("INTERCOM", env!("CARGO_PKG_VERSION"));
}

fn validate_firmware_target() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.ends_with("-none-elf") {
        return;
    }

    let c6 = std::env::var_os("CARGO_FEATURE_ESP32C6").is_some();
    let s31 = std::env::var_os("CARGO_FEATURE_ESP32S31").is_some();
    let s3 = std::env::var_os("CARGO_FEATURE_ESP32S3").is_some();
    if let Err(error) = target_selection::select_target(c6, s31, s3, &target) {
        use target_selection::TargetSelectionError;
        match error {
            TargetSelectionError::NoChipSelected => panic!(
                "bare-metal target {target} requires exactly one chip feature: esp32c6, esp32s31, or esp32s3"
            ),
            TargetSelectionError::MultipleChipsSelected => panic!(
                "bare-metal target {target} requires exactly one chip feature; multiple chip features are enabled"
            ),
            TargetSelectionError::TargetMismatch { expected, .. } => panic!(
                "selected chip requires Rust target {expected}, but Cargo selected {target}"
            ),
        }
    }
}
