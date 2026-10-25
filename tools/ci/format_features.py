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
# The Arrow crates of the table hand-off, which the arrow feature selects and
# Parquet needs too, and the encoding of the Arrow schema that a Parquet file
# embeds, which only Parquet reads (#3696).
ARROW = ("arrow-array", "arrow-buffer", "arrow-data", "arrow-schema")
ARROW_SCHEMA = ("arrow-ipc", "flatbuffers")
# The default features without Parquet: a graph that keeps the Arrow table
# hand-off and leaves out the embedded-schema encoding.
WITHOUT_PARQUET = ["yaml", "readstat", "odm", "arrow"]


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


def selected(enabled):
    """The facade's feature options for `enabled`, without the defaults."""
    features = ["--no-default-features"]
    if enabled:
        features += ["--features", ",".join(enabled)]
    return features


def graph(enabled):
    """The packages of the facade's Cargo graph with `enabled` features."""
    return packages(
        cargo(
            "tree",
            "--locked",
            "-p",
            "yamaa",
            *selected(enabled),
            "--prefix",
            "none",
            "--format",
            "{p}",
            capture=True,
        )
    )


def check_arrow(selection, tree, enabled):
    """Arrow comes with the arrow or parquet feature, and its IPC encoding
    and flatbuffers with parquet alone."""
    for package in ARROW:
        if (package in tree) != bool({"arrow", "parquet"}.intersection(enabled)):
            raise RuntimeError(f"unexpected {package} dependency for {selection}")
    for package in ARROW_SCHEMA:
        if (package in tree) != ("parquet" in enabled):
            raise RuntimeError(f"unexpected {package} dependency for {selection}")


def qualify_without_parquet():
    """The default features less Parquet keep the Arrow table hand-off without
    its IPC encoding, and the facade compiles with them."""
    print("Checking default-without-parquet", flush=True)
    check_arrow("default-without-parquet", graph(WITHOUT_PARQUET), WITHOUT_PARQUET)
    cargo("check", "--locked", "-p", "yamaa", *selected(WITHOUT_PARQUET))
    print("default-without-parquet: Cargo graph and facade build passed", flush=True)


def qualify(selection, enabled):
    print(f"Checking {selection}", flush=True)
    features = selected(enabled)
    tree = graph(enabled)
    check_arrow(selection, tree, enabled)
    # The native Parquet reader and writer need no parquet crate; only the
    # adapters' differential harness, a test build, compiles it (#3878).
    if "parquet" in tree:
        raise RuntimeError(f"unexpected parquet dependency for {selection}")
    for package, feature in [
        ("yamaa-saphyr-parser", "yaml"),
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
    # The adapters' unit tests behind a disabled reader, such as the
    # sas_source refusal without readstat, compile in no other build.
    cargo("test", "--locked", "-p", "yamaa-adapters", *features, "--lib", "--no-run")
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
    qualify_without_parquet()


if __name__ == "__main__":
    main()
