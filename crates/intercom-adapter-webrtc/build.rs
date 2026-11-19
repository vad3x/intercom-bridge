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
        .flag_if_supported("-O3")
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
    let new_mbed_build = |opt_flag: &str| {
        let mut build = cc::Build::new();
        build
            .include(mbedtls_dir.join("include"))
            .include(mbedtls_dir.join("library"))
            .include(&csrc_dir)
            .define("MBEDTLS_CONFIG_FILE", "\"mbedtls_intercom_config.h\"")
            .flag_if_supported(opt_flag)
            .flag_if_supported("-Wno-unused-parameter");
        build
    };
    let mut mbed = new_mbed_build("-O2");
    let mut mbed_ecc = new_mbed_build("-O3");

    mbed.file(csrc_dir.join("dtls_intercom.c"));

    for entry in std::fs::read_dir(mbedtls_dir.join("library")).expect("vendored mbedtls dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("c") {
            let name = path.file_name().unwrap().to_str().unwrap();
            if name.starts_with("net_sockets") || name.starts_with("timing") {
                continue;
            }
            if matches!(
                name,
                "bignum.c"
                    | "bignum_core.c"
                    | "bignum_mod.c"
                    | "bignum_mod_raw.c"
                    | "ecdh.c"
                    | "ecdsa.c"
                    | "ecp.c"
                    | "ecp_curves.c"
                    | "ecp_curves_new.c"
                    | "psa_crypto_ecp.c"
            ) {
                mbed_ecc.file(path);
            } else {
                mbed.file(path);
            }
        }
    }
    mbed.compile("mbedtls_dtls_srtp");
    mbed_ecc.compile("mbedtls_dtls_srtp_ecc");
}
