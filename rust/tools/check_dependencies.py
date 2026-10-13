"""Reject dependencies that cross the workspace's architecture boundaries."""

import json
import re
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]
ALLOWED = {
    "yamaa-core": {"ryu", "libm", "num-bigint"},
    "yamaa-engine": {"yamaa-core"},
    "yamaa-adapters": {
        "yamaa-core",
        "yamaa-engine",
        "serde",
        "serde_json",
        "toml",
        "saphyr-parser",
        "num-bigint",
        "arrow-array",
        "arrow-schema",
        "arrow-buffer",
        "arrow-ipc",
        "flatbuffers",
        "parquet",
        "bytes",
        "base64",
        "brotli",
        "flate2",
        "lz4_flex",
        "zstd",
        "snap",
        "rustix",
        "windows-sys",
    },
    "yamaa-python": {
        "yamaa-engine",
        "yamaa-adapters",
        "pyo3",
        "pyo3-build-config",
    },
    "yamaa-r": {"yamaa-engine", "yamaa-adapters", "extendr-api"},
}


WINDOWS_FILE_FEATURES = [
    "Wdk_Foundation",
    "Wdk_Storage_FileSystem",
    "Win32_Foundation",
    "Win32_Security",
    "Win32_Storage_FileSystem",
    "Win32_System_IO",
    "Win32_System_Threading",
    "Win32_Security_Authorization",
    "Win32_Globalization",
]


def violations(metadata):
    packages = {p["name"]: p for p in metadata["packages"]}
    errors = []
    if set(packages) != set(ALLOWED):
        errors.append("workspace members differ from the five reviewed crates")
    for name, package in packages.items():
        for dependency in package["dependencies"]:
            if dependency["name"] not in ALLOWED.get(name, set()):
                errors.append(f"{name} must not depend on {dependency['name']}")
            if dependency["name"] == "rustix" and (
                name != "yamaa-adapters"
                or dependency.get("target") != "cfg(unix)"
                or dependency.get("kind") is not None
                or dependency.get("req") != "=1.1.5"
                or dependency.get("features") != ["fs", "process"]
            ):
                errors.append(
                    "rustix is restricted to the pinned Unix filesystem adapter"
                )
            if dependency["name"] == "windows-sys" and (
                name != "yamaa-adapters"
                or dependency.get("target") != "cfg(windows)"
                or dependency.get("kind") is not None
                or dependency.get("req") != "=0.61.2"
                or dependency.get("features") != WINDOWS_FILE_FEATURES
            ):
                errors.append(
                    "windows-sys is restricted to the pinned Windows filesystem adapter"
                )
    return errors


def adapter_source_violations(source, identity):
    """Resolve imported core names sufficiently to guard semantic parser/compiler calls."""
    errors = []
    # Reviewed codec/admission adapters may name core values and diagnostics;
    # semantic compilation, parsing and validation must enter through the engine.
    semantic = r"(?:compile|parse|validate|analyze)(?:_[A-Za-z0-9_]+)?"
    imports = re.findall(r"\b(?:pub\s+)?use\s+yamaa_core\s*::([\s\S]*?);", source)
    core_names = {"yamaa_core"}
    semantic_names = set()
    for tree in imports:
        for name, alias in re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)(?:\s+as\s+([A-Za-z_][A-Za-z0-9_]*))?", tree):
            if name == "as":
                continue
            core_names.add(alias or name)
            if re.fullmatch(semantic, name):
                semantic_names.add(alias or name)
    for name in core_names:
        if re.search(r"\b" + re.escape(name) + r"\s*::(?:\s*[A-Za-z_][A-Za-z0-9_]*\s*::)*\s*" + semantic + r"\s*\(", source):
            errors.append(f"{identity}: adapter semantic calls must delegate to the engine")
            break
    if any(re.search(r"\b"+re.escape(name)+r"\s*\(",source) for name in semantic_names):
        errors.append(f"{identity}: imported core semantic calls must delegate to the engine")
    if re.search(r"\.\s*(?:validate_header|validate_output|validate_document|validate_descriptor|validate_types|parse_query_types|compile_bundle)\s*\(", source):
        errors.append(f"{identity}: adapter semantic methods must delegate to the engine")
    if re.search(r'\b(?:pub\s+)?mod\s+[A-Za-z_]+_transport\s*;',source):
        errors.append(f"{identity}: retired component request protocols must not return")
    return errors


def source_violations(workspace):
    errors = []
    for crate in ("yamaa-python", "yamaa-r"):
        for path in sorted((workspace / "crates" / crate / "src").rglob("*.rs")):
            source = path.read_text(encoding="utf-8")
            if re.search(r"\byamaa_core\s*::", source):
                errors.append(f"{path}: bindings must use application port types")
            if re.search(r"\b(?:65_?536|16_777_216|67_108_864)\b", source):
                errors.append(f"{path}: host capacity policy belongs in adapters")
            if re.search(r"\.is_(?:finite|nan|infinite)\s*\(", source):
                errors.append(f"{path}: scalar normalization belongs in core")
    for path in sorted((workspace / "crates/yamaa-adapters/src").rglob("*.rs")):
        errors.extend(adapter_source_violations(path.read_text(encoding="utf-8"), path))
    return errors


def main():
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=WORKSPACE,
            text=True,
        )
    )
    errors = violations(metadata) + source_violations(WORKSPACE)
    if errors:
        raise SystemExit("\n".join(errors))
    print("Rust dependency boundaries passed")


if __name__ == "__main__":
    main()
