"""Repository IO and lossless codecs around installed shared schema facts.

This module captures an explicit schema closure. Rust owns descriptor admission,
type validation, defaults and normalization; no reference interpreter is used.
"""

import json
import stat
from collections import deque
from pathlib import Path

from yamaa._native import _capture_schema_metadata, _schema_include_names

import yaml

ROOT_CLASSES = {
    "schema.yaml": "root_class",
    "schema_environment.yaml": "environment_class",
    "schema_define.yaml": "define_class",
}


def _integer(text):
    """Decode exact native decimal text without changing Python's global limit."""
    negative = text.startswith("-")
    digits = text[1:] if negative else text
    result = 0
    for offset in range(0, len(digits), 9):
        chunk = digits[offset : offset + 9]
        result = result * 10 ** len(chunk) + int(chunk)
    return -result if negative else result


def _integer_text(value):
    negative = value < 0
    value = abs(value)
    chunks = []
    while value:
        value, chunk = divmod(value, 1_000_000_000)
        chunks.append(chunk)
    if not chunks:
        return "0"
    text = str(chunks.pop()) + "".join(f"{chunk:09d}" for chunk in reversed(chunks))
    return "-" + text if negative else text


class _Dumper(yaml.SafeDumper):
    pass


def _represent_integer(dumper, value):
    text = _integer_text(value)
    # Native radix admission counts authored digits; a wide admitted hex value
    # can exceed that decimal-digit count while retaining its exact value.
    if value >= 0 and len(text) > 4096:
        text = hex(value)
    return dumper.represent_scalar("tag:yaml.org,2002:int", text)


_Dumper.add_representer(int, _represent_integer)


def decode_document(encoded):
    """Decode tagged native values without interpreting their schema or meaning."""
    arena = json.loads(encoded)
    nodes = arena["nodes"]

    def value(index):
        node = nodes[index]
        kind = node[0]
        if kind == "null":
            return None
        if kind == "int":
            return _integer(node[1])
        if kind in {"str", "float", "bool"}:
            return node[1]
        if kind == "list":
            return [value(child) for child in node[1]]
        if kind == "dict":
            return {value(key): value(child) for key, child in node[1]}
        raise RuntimeError(f"unknown native metadata scalar kind: {kind!r}")

    return value(arena["root"])


def capture_modules(directory, entrypoint):
    """Bound confined IO before native include discovery and final admission."""
    directory = Path(directory).resolve(strict=True)
    pending, captured, attempted = deque([entrypoint]), {}, set()
    byte_count = identity_count = 0
    while pending:
        name = pending.popleft()
        if name in attempted:
            continue
        if len(attempted) >= 128:
            raise ValueError("schema module capture limit")
        attempted.add(name)
        identity_count += len(name.encode("utf-8"))
        if identity_count > 65_536:
            raise ValueError("schema identity capture limit")
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            if name == entrypoint:
                raise ValueError("schema entry is outside its declared directory")
            continue
        path = directory / name
        try:
            ancestor = directory
            for segment in relative.parts:
                ancestor = ancestor / segment
                if stat.S_ISLNK(ancestor.lstat().st_mode):
                    raise OSError(f"schema capture contains a symbolic link: {name}")
            with path.open("rb") as stream:
                source = stream.read(16_777_216 - byte_count + 1)
        except FileNotFoundError:
            if name == entrypoint:
                raise
            continue
        byte_count += len(source)
        if byte_count > 16_777_216:
            raise ValueError("schema byte capture limit")
        captured[name] = source
        pending.extend(_schema_include_names(source, str(path)))
    return list(captured.items())


def capture(directory, entrypoint="schema.yaml"):
    return _capture_schema_metadata(
        capture_modules(directory, entrypoint),
        0,
        ROOT_CLASSES.get(entrypoint, "root_class"),
    )


def schema_errors(error, label):
    try:
        defects = json.loads(str(error))["schema_defects"]
    except (ValueError, TypeError, KeyError):
        return [f"ERROR: {label}: {error}"]
    return [
        f"ERROR: {Path(label).parent / item.get('identity', Path(label).name)}."
        f"{item['path']}: {item['kind']}"
        + (
            f" (authored {item['authored']!r})"
            if item.get("authored") is not None
            else ""
        )
        for item in defects
    ]


def fixture(environment):
    """Capture the schema declarations an independent partial control authors."""
    if "native" in environment:
        return environment["native"]
    authored = {
        "version": environment.get("version", "1.0"),
        "root_class": [],
        **environment.get("classes", {}),
        **environment.get("aliases", {}),
        **environment.get("registries", {}),
    }
    return _capture_schema_metadata(
        [("schema.yaml", bytes_for(authored))],
        0,
        "root_class",
    )


def bytes_for(value):
    return yaml.dump(value, Dumper=_Dumper, allow_unicode=False).encode("ascii")


def failure_rows(error):
    """Only a complete native semantic response can be a language finding."""
    try:
        observed = json.loads(str(error))["outcome"]
    except (ValueError, TypeError, KeyError):
        raise error
    if observed.get("status") != "invalid":
        raise error
    return [
        (
            row["phase"],
            row["condition"],
            row["requirement"],
            row["spec_paths"],
            json.dumps(row["context"]),
        )
        for row in observed["diagnostics"]
    ]
