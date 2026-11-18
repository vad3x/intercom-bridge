//! Chip feature and Rust target compatibility checks.

/// Hardware chip selected for a firmware build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetChip {
    Esp32C6,
    Esp32S31,
    Esp32S3,
}

/// Why a feature and target triple cannot produce a supported firmware image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetSelectionError {
    NoChipSelected,
    MultipleChipsSelected,
    TargetMismatch {
        chip: TargetChip,
        expected: &'static str,
    },
}

/// Validate one selected chip feature against its required Rust target.
pub fn select_target(
    esp32c6: bool,
    esp32s31: bool,
    esp32s3: bool,
    rust_target: &str,
) -> Result<TargetChip, TargetSelectionError> {
    let selected = match (esp32c6, esp32s31, esp32s3) {
        (false, false, false) => return Err(TargetSelectionError::NoChipSelected),
        (true, false, false) => (TargetChip::Esp32C6, "riscv32imac-unknown-none-elf"),
        (false, true, false) => (TargetChip::Esp32S31, "riscv32imafc-unknown-none-elf"),
        (false, false, true) => (TargetChip::Esp32S3, "xtensa-esp32s3-none-elf"),
        _ => return Err(TargetSelectionError::MultipleChipsSelected),
    };

    if rust_target != selected.1 {
        return Err(TargetSelectionError::TargetMismatch {
            chip: selected.0,
            expected: selected.1,
        });
    }

    Ok(selected.0)
}

#[cfg(test)]
mod tests {
    use super::{select_target, TargetChip, TargetSelectionError};

    #[test]
    fn chip_features_match_rust_targets() {
        // Arrange
        let expected = [
            (
                true,
                false,
                false,
                "riscv32imac-unknown-none-elf",
                TargetChip::Esp32C6,
            ),
            (
                false,
                true,
                false,
                "riscv32imafc-unknown-none-elf",
                TargetChip::Esp32S31,
            ),
            (
                false,
                false,
                true,
                "xtensa-esp32s3-none-elf",
                TargetChip::Esp32S3,
            ),
        ];

        // Act
        let actual = expected.map(|(c6, s31, s3, target, _)| select_target(c6, s31, s3, target));

        // Assert
        assert_eq!(actual, expected.map(|(_, _, _, _, chip)| Ok(chip)));

        // Act
        let no_chip = select_target(false, false, false, "x86_64-apple-darwin");
        let multiple_chips = select_target(true, false, true, "riscv32imac-unknown-none-elf");
        let mismatched_target = select_target(true, false, false, "xtensa-esp32s3-none-elf");

        // Assert
        assert_eq!(no_chip, Err(TargetSelectionError::NoChipSelected));
        assert_eq!(
            multiple_chips,
            Err(TargetSelectionError::MultipleChipsSelected)
        );
        assert_eq!(
            mismatched_target,
            Err(TargetSelectionError::TargetMismatch {
                chip: TargetChip::Esp32C6,
                expected: "riscv32imac-unknown-none-elf",
            })
        );
    }
}
