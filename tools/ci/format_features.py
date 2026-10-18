"""Qualify one independent adapter format selection and its native dependency edges."""

import argparse
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "rust/Cargo.toml"
FORMATS = {
    "none": [],
    "yaml": ["yaml"],
    "parquet": ["parquet"],
    "readstat": ["readstat"],
    "odm": ["odm"],
    "all": ["yaml", "parquet", "readstat", "odm"],
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("selection", choices=FORMATS)
    args = parser.parse_args()
    enabled = FORMATS[args.selection]
    features = ["--no-default-features"]
    if enabled:
        features += ["--features", ",".join(enabled)]
    tree = packages(
        cargo(
            "tree",
            "--locked",
            "-p",
            "yamaa-adapters",
            *features,
            "--prefix",
            "none",
            "--format",
            "{p}",
            capture=True,
        )
    )
    for package, feature in [
        ("saphyr-parser", "yaml"),
        ("parquet", "parquet"),
        ("zstd-sys", "parquet"),
        ("quick-xml", "odm"),
    ]:
        if (package in tree) != (feature in enabled):
            raise RuntimeError(f"unexpected {package} dependency for {args.selection}")
    if ("flate2" in tree) != bool({"parquet", "odm"}.intersection(enabled)):
        raise RuntimeError(f"unexpected flate2 dependency for {args.selection}")
    if ("cc" in tree) != bool({"parquet", "readstat"}.intersection(enabled)):
        raise RuntimeError(f"unexpected native C build dependency for {args.selection}")
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
            "saphyr-parser",
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
    cargo("test", "--locked", "-p", "yamaa-adapters", *features)
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
            raise RuntimeError(
                f"unexpected linked {feature} symbols for {args.selection}"
            )
    print(f"{args.selection}: fixtures, Cargo graph and native symbols passed")


if __name__ == "__main__":
    main()
