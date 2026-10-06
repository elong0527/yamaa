"""Explicit shared-Rust schema loading with the existing host specification pipeline.

The Python public loader remains the default. This adapter owns filesystem/YAML
IO and modeling; the captured native snapshot owns schema interpretation.
"""

from __future__ import annotations

import re
from pathlib import Path

from yamaa.adapters._native_schema_findings import admission_error
from yamaa.adapters._native_schema_interpreter import NativeSchemaInterpreter
from yamaa.adapters._native_schema_wire import (
    decode_nodes,
    encode_tree,
    limit,
    request,
    response,
)
from yamaa.specification._yaml import read_yaml_bytes
from yamaa.specification.loader import load_specification_with_bundle
from yamaa.specification.schema import SchemaBundle, _schema_failure

_SAFE_INCLUDE = re.compile(r"schema_[a-z0-9_]+\.yaml\Z")


def load_schema_bundle(
    schema_root: str | Path,
    *,
    entry_name: str = "schema.yaml",
    root_class: str = "root_class",
    native=None,
) -> SchemaBundle:
    """Capture the native service before reading a confined YAML source closure."""
    if native is None:
        import yamaa_native as native
    compile_schema = getattr(native, "_compile_schema", None)
    if not callable(compile_schema):
        raise TypeError("native _compile_schema must be callable")
    root = Path(schema_root).resolve()
    entrypoint = root / entry_name
    if entry_name != "schema.yaml" and not _SAFE_INCLUDE.fullmatch(entry_name):
        raise _schema_failure(entrypoint, f"unsafe schema entry {entry_name!r}")
    if entrypoint.is_symlink() or not entrypoint.is_file():
        raise _schema_failure(entrypoint, f"{entry_name} is not a regular file")
    modules, decoded = [], []
    pending, seen = [entrypoint], set()
    source_bytes = 0
    while pending:
        current = pending.pop()
        if current.name in seen:
            continue
        seen.add(current.name)
        if len(seen) > 128:
            limit("bundle", "modules", 128)
        # Includes have only basenames; reject symlinks before any file read.
        # This is the same host authority boundary as the default YAML loader.
        if current.is_symlink() or not current.is_file():
            continue  # Rust reports the absent declared include.
        try:
            current.resolve().relative_to(root)
        except ValueError as error:
            raise _schema_failure(
                current, f"unsafe include {current.name!r}"
            ) from error
        with current.open("rb") as stream:
            raw = stream.read(8_388_609 - source_bytes)
        source_bytes += len(raw)
        if source_bytes > 8_388_608:
            limit("yaml_source", "bytes", 8_388_608)
        document = read_yaml_bytes(raw, current)
        tree = encode_tree(document)
        modules.append({"name": current.name, "document": tree})
        decoded.append(decode_nodes(tree))
        if isinstance(document, dict) and isinstance(document.get("includes"), list):
            # Only discover safe source names here. Rust admits include syntax,
            # cycles, versions, declarations and the complete closure.
            for include in document["includes"]:
                if type(include) is str and _SAFE_INCLUDE.fullmatch(include):
                    pending.append(current.parent / include)
    snapshot, text = compile_schema(
        request(
            {
                "schema": {
                    "modules": modules,
                    "entry": 0,
                    "root_class": root_class,
                }
            }
        )
    )
    metadata = response(text)
    if metadata["status"] != "compiled":
        raise admission_error(
            metadata, entrypoint, decoded, [m["name"] for m in modules]
        )
    if snapshot is None or not callable(getattr(snapshot, "analyze", None)):
        raise ValueError("native schema admission lacks an owned snapshot")
    interpreter = NativeSchemaInterpreter(snapshot, metadata, decoded)
    classes, aliases, registries = interpreter.metadata()
    return SchemaBundle(
        version=metadata["version"],
        path=entrypoint,
        classes=classes,
        aliases=aliases,
        registries=registries,
        interpreter=interpreter,
    )


def load_specification(entry_path: str | Path, schema_root: str | Path, *, native=None):
    """Opt into shared schema interpretation throughout one host loading pipeline."""
    bundle = load_schema_bundle(schema_root, native=native)
    return load_specification_with_bundle(entry_path, bundle)
