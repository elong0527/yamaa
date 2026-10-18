"""Smoke-test minimal Rust format builds and their native dependency edges."""

import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "rust/Cargo.toml"
FORMATS = {
    "none": [],
    "yaml-parquet": ["yaml", "parquet"],
}


def cargo(*args, capture=False):
    return subprocess.run(
        ["cargo", *args, "--manifest-path", str(MANIFEST)],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    ).stdout


def packages(tree):
    return {line.split()[0] for line in tree.splitlines() if line.strip()}


def qualify(selection, enabled):
    print(f"Checking {selection}", flush=True)
    features = ["--no-default-features"]
    if enabled:
        features += ["--features", ",".join(enabled)]
    tree = packages(
        cargo(
            "tree",
            "--locked",
            "-p",
            "yamaa",
            *features,
            "--prefix",
            "none",
            "--format",
            "{p}",
            capture=True,
        )
    )
    for package, feature in [
        ("yamaa-saphyr-parser", "yaml"),
        ("parquet", "parquet"),
        ("zstd-sys", "parquet"),
        ("quick-xml", "odm"),
    ]:
        if (package in tree) != (feature in enabled):
            raise RuntimeError(f"unexpected {package} dependency for {selection}")
    if ("flate2" in tree) != bool({"parquet", "odm"}.intersection(enabled)):
        raise RuntimeError(f"unexpected flate2 dependency for {selection}")
    if ("cc" in tree) != bool({"parquet", "readstat"}.intersection(enabled)):
        raise RuntimeError(f"unexpected native C build dependency for {selection}")
    cargo("check", "--locked", "-p", "yamaa", *features)
    messages = cargo(
        "test",
        "--locked",
        "-p",
        "yamaa-adapters",
        *features,
        "--test",
        "format_features",
        "--no-run",
        "--message-format=json",
        capture=True,
    )
    executables = [
        item["executable"]
        for line in messages.splitlines()
        if (item := json.loads(line)).get("executable")
    ]
    if len(executables) != 1:
        raise RuntimeError("expected one capability test executable")
    symbols = subprocess.check_output(
        ["nm", "--defined-only", executables[0]], text=True
    )
    for expression, feature in [
        (r"\breadstat_\w+", "readstat"),
        (r"\bZSTD_\w+", "parquet"),
    ]:
        if bool(re.search(expression, symbols)) != (feature in enabled):
            raise RuntimeError(f"unexpected linked {feature} symbols for {selection}")
    subprocess.run([executables[0]], cwd=ROOT, check=True)
    print(f"{selection}: fixtures, Cargo graph and native symbols passed", flush=True)


def main():
    core = packages(
        cargo(
            "tree",
            "--locked",
            "-p",
            "yamaa-core",
            "--prefix",
            "none",
            "--format",
            "{p}",
            capture=True,
        )
    )
    if core.intersection(
        {
            "yamaa-saphyr-parser",
            "parquet",
            "arrow-array",
            "arrow-ipc",
            "cc",
            "zstd-sys",
            "quick-xml",
            "flate2",
        }
    ):
        raise RuntimeError("semantic core depends on a physical reader")
    for selection, enabled in FORMATS.items():
        qualify(selection, enabled)


if __name__ == "__main__":
    main()
