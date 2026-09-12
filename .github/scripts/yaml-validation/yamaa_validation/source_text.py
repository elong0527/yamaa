from pathlib import Path


ASCII_SOURCE_SUFFIXES = {
    ".csv",
    ".json",
    ".md",
    ".py",
    ".r",
    ".rb",
    ".rd",
    ".sh",
    ".toml",
    ".txt",
    ".yaml",
    ".yml",
}
ASCII_SOURCE_NAMES = {"DESCRIPTION", "NAMESPACE"}
ASCII_SOURCE_IGNORED_PARTS = {
    ".git",
    ".pytest_cache",
    ".venv",
    "__pycache__",
    "venv",
}


def is_unicode_fixture_csv(relative: Path):
    parts = relative.parts
    return (
        relative.suffix.lower() == ".csv"
        and len(parts) >= 5
        and parts[0] == "yaml"
        and parts[1] == "examples"
        and parts[3] in {"input", "expected"}
    )


def validate_ascii_sources(root: Path):
    errors = []
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(root)
        if any(part in ASCII_SOURCE_IGNORED_PARTS for part in relative.parts):
            continue
        if is_unicode_fixture_csv(relative):
            continue
        if (
            path.suffix.lower() not in ASCII_SOURCE_SUFFIXES
            and path.name not in ASCII_SOURCE_NAMES
        ):
            continue

        try:
            content = path.read_bytes()
        except OSError as exc:
            errors.append(f"ERROR: {relative}: cannot read source: {exc}")
            continue

        for offset, value in enumerate(content):
            if value <= 0x7F:
                continue
            line = content.count(b"\n", 0, offset) + 1
            previous_newline = content.rfind(b"\n", 0, offset)
            column = offset - previous_newline
            errors.append(
                f"ERROR: {relative}:{line}:{column}: non_ascii_source "
                f"byte 0x{value:02X}"
            )
            break
    return errors


def diagnostic_path_key(key):
    return str(key).encode("unicode_escape").decode("ascii")


def validate_unicode_scalars(value, path):
    errors = []
    if isinstance(value, str):
        for index, character in enumerate(value):
            code_point = ord(character)
            if 0xD800 <= code_point <= 0xDFFF:
                errors.append(
                    f"ERROR: {path}: invalid_text surrogate U+{code_point:04X} "
                    f"at string offset {index}"
                )
                break
    elif isinstance(value, list):
        for index, item in enumerate(value):
            errors.extend(validate_unicode_scalars(item, f"{path}[{index}]"))
    elif isinstance(value, dict):
        for key, item in value.items():
            errors.extend(validate_unicode_scalars(key, f"{path}.<key>"))
            errors.extend(
                validate_unicode_scalars(
                    item,
                    f"{path}.{diagnostic_path_key(key)}",
                )
            )
    return errors
