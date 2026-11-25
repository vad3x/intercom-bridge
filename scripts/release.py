#!/usr/bin/env python3
"""Bump [workspace.package].version in Cargo.toml, commit, and create an annotated vX.Y.Z tag."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

SEMVER_RE = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")
WORKSPACE_VERSION_RE = re.compile(
    r'(\[workspace\.package\][^\[]*?\nversion\s*=\s*)"[^"]*"',
    re.DOTALL,
)


def parse_semver(version: str) -> tuple[int, int, int]:
    match = SEMVER_RE.match(version.strip())
    if not match:
        raise ValueError(f"Invalid SemVer '{version}': expected MAJOR.MINOR.PATCH")
    major, minor, patch = (int(g) for g in match.groups())
    if major > 42:
        raise ValueError(f"Major version {major} exceeds Matter u32 limit (max 42)")
    if minor >= 100 or patch >= 100:
        raise ValueError(f"Minor/patch ({minor}.{patch}) must be < 100")
    return major, minor, patch


def is_working_tree_clean(repo: Path) -> bool:
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() == ""


def run_git(cmd: list[str], cwd: Path) -> None:
    subprocess.run(cmd, cwd=cwd, check=True)


def update_cargo_toml_version(cargo_toml: Path, version: str) -> None:
    content = cargo_toml.read_text(encoding="utf-8")
    updated, count = WORKSPACE_VERSION_RE.subn(rf'\1"{version}"', content, count=1)
    if count != 1:
        raise RuntimeError(f"Could not find [workspace.package] version in {cargo_toml}")
    cargo_toml.write_text(updated, encoding="utf-8")


def run_release(repo: Path, version_arg: str, dry_run: bool = False) -> str:
    major, minor, patch = parse_semver(version_arg)
    version = f"{major}.{minor}.{patch}"
    tag = f"v{version}"

    if not is_working_tree_clean(repo):
        raise RuntimeError("Working tree is dirty; commit or stash changes before releasing.")

    cargo_toml = repo / "Cargo.toml"
    if dry_run:
        print(f"[dry-run] Would update {cargo_toml} to version = \"{version}\"")
        print(f"[dry-run] Would run: git add Cargo.toml")
        print(f"[dry-run] Would run: git commit -m \"chore(release): {tag}\"")
        print(f"[dry-run] Would run: git tag -a {tag} -m \"Release {tag}\"")
        return tag

    update_cargo_toml_version(cargo_toml, version)
    run_git(["git", "add", "Cargo.toml"], repo)
    diff_result = subprocess.run(
        ["git", "diff", "--cached", "--quiet"],
        cwd=repo,
    )
    if diff_result.returncode != 0:
        run_git(["git", "commit", "-m", f"chore(release): {tag}"], repo)
    run_git(["git", "tag", "-a", tag, "-m", f"Release {tag}"], repo)
    return tag


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Create a SemVer release commit and annotated tag.")
    parser.add_argument("version", help="Semantic version (e.g. 2.0.0 or v2.0.0)")
    parser.add_argument("--dry-run", action="store_true", help="Print actions without modifying files or git")
    args = parser.parse_args(argv)

    repo = Path(__file__).resolve().parent.parent
    try:
        tag = run_release(repo, args.version, dry_run=args.dry_run)
        if not args.dry_run:
            print(f"Created release {tag}")
        return 0
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
