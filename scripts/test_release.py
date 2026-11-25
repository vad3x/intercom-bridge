import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import release


class ReleaseScriptTests(unittest.TestCase):
    def test_validate_semver_accepts_valid_and_rejects_invalid(self):
        self.assertEqual(release.parse_semver("2.0.0"), (2, 0, 0))
        self.assertEqual(release.parse_semver("v1.0.3"), (1, 0, 3))
        with self.assertRaises(ValueError):
            release.parse_semver("1.2")
        with self.assertRaises(ValueError):
            release.parse_semver("0.100.0")
        with self.assertRaises(ValueError):
            release.parse_semver("43.0.0")

    def test_update_cargo_toml_version_replaces_workspace_version(self):
        with tempfile.TemporaryDirectory() as tmp:
            cargo_toml = Path(tmp) / "Cargo.toml"
            cargo_toml.write_text(
                '[workspace.package]\nversion = "0.1.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            release.update_cargo_toml_version(cargo_toml, "2.0.0")
            updated = cargo_toml.read_text(encoding="utf-8")
            self.assertIn('version = "2.0.0"', updated)
            self.assertNotIn('version = "0.1.0"', updated)

    def test_run_release_rejects_dirty_working_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            (repo / "Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.1.0"\n', encoding="utf-8"
            )
            with mock.patch.object(release, "is_working_tree_clean", return_value=False):
                with self.assertRaises(RuntimeError):
                    release.run_release(repo, "2.0.0", dry_run=False)

    def test_run_release_builds_expected_git_commands(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            (repo / "Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.1.0"\n', encoding="utf-8"
            )
            calls = []

            def fake_run(cmd, cwd):
                calls.append((cmd, cwd))

            with mock.patch.object(release, "is_working_tree_clean", return_value=True):
                with mock.patch.object(release, "run_git", side_effect=fake_run):
                    with mock.patch.object(
                        release.subprocess,
                        "run",
                        return_value=mock.Mock(returncode=1),
                    ):
                        release.run_release(repo, "2.0.0", dry_run=False)

            self.assertEqual(
                calls,
                [
                    (["git", "add", "Cargo.toml"], repo),
                    (["git", "commit", "-m", "chore(release): v2.0.0"], repo),
                    (["git", "tag", "-a", "v2.0.0", "-m", "Release v2.0.0"], repo),
                ],
            )


if __name__ == "__main__":
    unittest.main()
