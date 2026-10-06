#!/usr/bin/env python3
"""Compute an idempotent release version and stamp the workspace for CI builds."""
import argparse
import datetime
import os
from pathlib import Path
import re
import subprocess
import tomllib
import xml.etree.ElementTree as ET

SEMVER = re.compile(r"v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)\Z")


def version_tuple(value):
    match = SEMVER.fullmatch(value)
    if not match:
        raise ValueError(f"Not a stable semantic version: {value!r}")
    return tuple(map(int, match.groups()))


def bump(base, messages):
    major, minor, patch = version_tuple(base)
    if any(re.search(r"(?m)^BREAKING[ -]CHANGE:\s|^\w+(?:\([^\n)]+\))?!:", text)
           for text in messages):
        return f"{major + 1}.0.0"
    if any(re.search(r"(?m)^feat(?:\([^\n)]+\))?:", text) for text in messages):
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def compute(sha, before):
    # Re-running the same commit must update its existing release, not create another.
    existing = [tag for tag in git("tag", "--points-at", sha).splitlines()
                if tag.startswith("v") and SEMVER.fullmatch(tag)]
    if existing:
        return max(existing, key=version_tuple)[1:]
    manifest = tomllib.loads(Path("Cargo.toml").read_text())
    candidates = [manifest["workspace"]["package"]["version"]]
    candidates += [tag for tag in git("tag", "--list", "v*").splitlines()
                   if SEMVER.fullmatch(tag)]
    base = max(candidates, key=version_tuple)
    # Use the actual push range, even if release tags are not ancestors (force push).
    revision = f"{before}..{sha}" if before and set(before) != {"0"} else sha
    messages = git("log", "--format=%B%x00", revision).split("\0")
    return bump(base, messages)


def stamp(root, version):
    version_tuple(version)
    manifest_path = root / "Cargo.toml"
    text = manifest_path.read_text()
    text, count = re.subn(r'(\[workspace\.package\]\s*\nversion\s*=\s*)"[^"]+"',
                         lambda m: f'{m[1]}"{version}"', text, count=1)
    if count != 1:
        raise ValueError("Cannot find workspace version")
    manifest_path.write_text(text)
    names = set()
    for path in (root / "crates").glob("*/Cargo.toml"):
        package = tomllib.loads(path.read_text())["package"]
        if package.get("version") == {"workspace": True}:
            names.add(package["name"])
    lock_path = root / "Cargo.lock"
    blocks = lock_path.read_text().split("[[package]]")
    changed = set()
    for index, block in enumerate(blocks[1:], 1):
        package = tomllib.loads(block)
        if package["name"] in names and "source" not in package:
            blocks[index] = re.sub(r'(?m)^version = "[^"]+"$',
                                   f'version = "{version}"', block, count=1)
            changed.add(package["name"])
    if changed != names:
        raise ValueError(f"Missing workspace packages in Cargo.lock: {names - changed}")
    lock_path.write_text("[[package]]".join(blocks))
    metadata = root / "packaging/flatpak/io.github.lvcky_gg.MtgoRs.metainfo.xml"
    if metadata.exists():
        tree = ET.parse(metadata)
        releases = tree.getroot().find("releases")
        if releases is None:
            raise ValueError("Missing AppStream releases element")
        releases.clear()
        ET.SubElement(releases, "release", version=version,
                      date=datetime.date.today().isoformat())
        ET.indent(tree, space="  ")
        tree.write(metadata, encoding="utf-8", xml_declaration=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    calculate = commands.add_parser("version")
    calculate.add_argument("--sha", required=True)
    calculate.add_argument("--before", default="")
    apply = commands.add_parser("stamp")
    apply.add_argument("version")
    apply.add_argument("--root", type=Path, default=Path("."))
    args = parser.parse_args()
    if args.command == "stamp":
        stamp(args.root, args.version)
    else:
        version = compute(args.sha, args.before)
        print(version)
        if output := os.environ.get("GITHUB_OUTPUT"):
            with open(output, "a") as file:
                file.write(f"version={version}\ntag=v{version}\n")


if __name__ == "__main__":
    main()
