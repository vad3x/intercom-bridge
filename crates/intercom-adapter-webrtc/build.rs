use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let opus_dir = manifest_dir.join("vendor/opus");
    let mbedtls_dir = manifest_dir.join("vendor/mbedtls");
    let csrc_dir = manifest_dir.join("csrc");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor");
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-changed=csrc/mbedtls_intercom_config.h");
    println!("cargo:rerun-if-changed=csrc/opus_no_malloc.h");

    // 1. Fixed-point zero-malloc libopus 1.5.2 (16 kHz VoIP SILK + CELT hybrid symbols)
    let mut opus = cc::Build::new();
    opus.include(opus_dir.join("include"))
        .include(opus_dir.join("celt"))
        .include(opus_dir.join("silk"))
        .include(opus_dir.join("silk/fixed"))
        .include(&csrc_dir)
        .define("OPUS_BUILD", None)
        .define("FIXED_POINT", "1")
        .define("DISABLE_FLOAT_API", "1")
        .define("VAR_ARRAYS", "1")
        .flag_if_supported("-Os")
        .flag_if_supported("-Wno-unused-parameter")
        .flag("-include")
        .flag(csrc_dir.join("opus_no_malloc.h").to_str().unwrap());

    for rel in [
        "src/opus.c",
        "src/opus_encoder.c",
        "src/opus_decoder.c",
        "src/repacketizer.c",
        "src/extensions.c",
    ] {
        opus.file(opus_dir.join(rel));
    }
    for dir in ["celt", "silk", "silk/fixed"] {
        for entry in std::fs::read_dir(opus_dir.join(dir)).expect("vendored opus dir") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) == Some("c")
                && path.file_name().unwrap() != "opus_custom_demo.c"
            {
                opus.file(path);
            }
        }
    }
    opus.compile("opus_fixed");

    // 2. Stripped bare-metal MbedTLS 3.6 LTS for DTLS 1.2 + RFC 5764 use_srtp
    let mut mbed = cc::Build::new();
    mbed.include(mbedtls_dir.join("include"))
        .include(mbedtls_dir.join("library"))
        .include(&csrc_dir)
        .define("MBEDTLS_CONFIG_FILE", "\"mbedtls_intercom_config.h\"")
        .flag_if_supported("-Os")
        .flag_if_supported("-Wno-unused-parameter");

    mbed.file(csrc_dir.join("dtls_intercom.c"));

    for entry in std::fs::read_dir(mbedtls_dir.join("library")).expect("vendored mbedtls dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("c") {
            let name = path.file_name().unwrap().to_str().unwrap();
            if !name.starts_with("net_sockets") && !name.starts_with("timing") {
                mbed.file(path);
            }
        }
    }
    mbed.compile("mbedtls_dtls_srtp");
}
