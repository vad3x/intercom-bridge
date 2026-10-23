use std::process::{Command, Output};

const MAC: &str = "98:a3:16:a6:78:44";
const QR: &str = "MT:-24J042C00X1VC4QK3P0OUYH0R92G1V.X12LJRQ19UZR1OADO1O0";

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_intercom-pairing-tool"))
        .args(args)
        .output()
        .unwrap()
}

fn assert_human_output(output: Output) {
    // Arrange
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Act
    let displayed = (
        stdout.contains("Smart Intercom — Matter Commissioning Credentials"),
        stdout.contains("IC-98A316A67844"),
        stdout.contains("3012-791-3250"),
        stdout.contains("7012-791-3256-55213-27699"),
        stdout.contains(QR),
    );

    // Assert
    assert!(output.status.success());
    assert_eq!(displayed, (true, true, true, true, true));
}

#[test]
fn mac_flag_displays_intercom_credentials() {
    // Arrange
    let args = ["--mac", MAC];

    // Act
    let output = run(&args);

    // Assert
    assert_human_output(output);
}

#[test]
fn short_mac_flag_displays_intercom_credentials() {
    // Arrange
    let args = ["-m", MAC];

    // Act
    let output = run(&args);

    // Assert
    assert_human_output(output);
}

#[test]
fn positional_mac_displays_intercom_credentials() {
    // Arrange
    let args = [MAC];

    // Act
    let output = run(&args);

    // Assert
    assert_human_output(output);
}

#[test]
fn json_flag_preserves_exact_output() {
    // Arrange
    let expected = concat!(
        "{\n",
        "  \"mac\": \"98:a3:16:a6:78:44\",\n",
        "  \"serial_number\": \"IC-98A316A67844\",\n",
        "  \"vendor_id\": 65521,\n",
        "  \"product_id\": 32769,\n",
        "  \"discriminator\": 3096,\n",
        "  \"passcode\": 21710079,\n",
        "  \"manual_code\": \"3012-791-3250\",\n",
        "  \"manual_code_long\": \"7012-791-3256-55213-27699\",\n",
        "  \"qr_code\": \"MT:-24J042C00X1VC4QK3P0OUYH0R92G1V.X12LJRQ19UZR1OADO1O0\"\n",
        "}\n"
    );

    // Act
    let output = run(&["--mac", MAC, "--json"]);

    // Assert
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
fn help_exits_successfully() {
    // Arrange
    let args = ["--help"];

    // Act
    let output = run(&args);

    // Assert
    assert!(output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("Usage: cargo qr"));
}

#[test]
fn short_mac_is_rejected() {
    // Arrange
    let args = ["--mac", "aa:bb"];

    // Act
    let output = run(&args);

    // Assert
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("MAC address must be 12 hex characters"));
}

#[test]
fn mac_formats_produce_identical_json() {
    // Arrange
    let colon = run(&["--mac", "98:a3:16:a6:78:44", "--json"]);
    let hyphen = run(&["--mac", "98-A3-16-A6-78-44", "--json"]);
    let compact = run(&["98A316A67844", "--json"]);

    // Act
    let outputs = [
        String::from_utf8(colon.stdout).unwrap(),
        String::from_utf8(hyphen.stdout).unwrap(),
        String::from_utf8(compact.stdout).unwrap(),
    ];

    // Assert
    assert!(colon.status.success());
    assert!(hyphen.status.success());
    assert!(compact.status.success());
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[1], outputs[2]);
    assert!(outputs[0].contains("\"mac\": \"98:a3:16:a6:78:44\""));
}

#[test]
fn cli_identity_matches_shared_device_identity() {
    // Arrange
    let mac_text = "98-A3-16-A6-78-44";
    let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];
    let identity =
        edf_matter::identity::derive_device_identity(&edf_matter::identity::IdentityParams {
            mac: &mac,
            serial_prefix: "IC",
            hostname_prefix: "intercom-bridge",
        })
        .unwrap();

    // Act
    let output = run(&["--mac", mac_text, "--json"]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Assert
    assert!(output.status.success());
    assert_eq!(identity.serial_str(), "IC-98A316A67844");
    assert_eq!(identity.hostname_str(), "intercom-bridge-7844");
    assert!(stdout.contains(&format!("\"serial_number\": \"{}\"", identity.serial_str())));
    assert!(stdout.contains(&format!("\"discriminator\": {}", identity.discriminator)));
    assert!(stdout.contains(&format!("\"passcode\": {}", identity.passcode)));
}
