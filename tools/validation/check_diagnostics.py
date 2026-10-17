"""Guard normative mappings and keep diagnostic literals in their registry.

Core registry conditions and phases must be in the shared vocabulary. Rule error
tables/refusal clauses must name registered conditions. A registered condition
needs no core cause or separate disposition record; the vocabulary and the core
registry are the only sources. These are static ownership checks, not a claim
that every registered condition has executable release coverage.
Condition and requirement literals outside the canonical registry, the generated
shipped schema and private unit tests are refused. A requirement is a legacy
REQ ID or a card ID, read from the shared rules model.
"""

import argparse
import json
import re
from pathlib import Path

from tools.paths import ROOT
from tools.rules.model import load, prose_lines

JSON_LITERAL = re.compile(r'"(?:[^"\\]|\\.)*"')
RAW_START = re.compile(r'(?:br|cr|r)(#*)"')
CHARACTER = re.compile(r"'(?:[^'\\\r\n]|\\(?:u\{[0-9a-fA-F_]+\}|[^\r\n]))'")
REGISTRATION = re.compile(
    r"^  ([a-z][a-z0-9_]*): \{phases: \[([a-z_, ]+)\]\}$", re.MULTILINE
)
HOST_CRATES = {"yamaa-adapters", "yamaa-python", "yamaa-r"}
CORE_CRATES = {"yamaa-core"}
REGISTRY = Path("rust/crates/yamaa-core/src/diagnostic/mod.rs")
SHIPPED_SCHEMA = Path("rust/crates/yamaa-adapters/src/shipped_schema/data.rs")
PRIVATE_TEST_MODULE = Path(
    "rust/crates/yamaa-adapters/src/file_define/publication_failure_tests.rs"
)
NULL_POLICY = Path("rules/execution/lifecycle.md")
NULL_START = "<!-- normative-null-conditions:start -->"
NULL_END = "<!-- normative-null-conditions:end -->"
NULL_ENTRY = re.compile(r"- `([a-z][a-z0-9_]*)` \(`([a-z_]+)`\)")
CONDITION_NAME = re.compile(r"[a-z][a-z0-9_]*\Z")
CONDITION_SPAN = r"`[a-z][a-z0-9_]*`"
CONDITION_LIST = (
    rf"{CONDITION_SPAN}(?:\s*(?:,\s*(?:(?:or|and)\s+)?|(?:or|and)\s+){CONDITION_SPAN})*"
)
REFUSAL = re.compile(
    rf"\b(?:fail(?:s|ed)?|refus(?:e|es|ed)|reject(?:s|ed)?|report(?:s|ed)?)\b"
    rf"(?:\s+(?:validation|evaluation|execution|derivation|ingestion|verification|output|parsing|mapping|before|during|at|in|under|the|entry|boundary|phase))*"
    rf"\s+with\s+({CONDITION_LIST})",
    re.IGNORECASE,
)
NAMED_CONDITION = re.compile(
    rf"\b(?:condition|diagnostic)\s+(?:named\s+)?({CONDITION_LIST})",
    re.IGNORECASE,
)


def cause_pattern(rules) -> re.Pattern:
    """One registry cause; its requirement is a legacy or card ID, or None."""
    return re.compile(
        r'\s*(\w+) => \("([a-z_]+)", "([a-z_]+)", '
        rf'(Some\("(?:{rules.identifier.pattern})"\)|None)\),\s*'
    )


def rule_condition_references(source: str) -> list[tuple[int, str]]:
    """Read actual condition columns and refusal prose, not arbitrary code spans.

    Fenced examples and HTML comments do not declare vocabulary. Refusal clauses
    are joined across Markdown paragraph wraps, including comma/or lists. A
    condition column explicitly declares its cells regardless of surrounding
    headings; field/operation columns and ordinary inline code remain opaque.
    """
    lines = prose_lines(source, code=True)
    result = []
    condition_column = None
    for index, line in enumerate(lines):
        if not line.lstrip().startswith("|"):
            condition_column = None
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        headers = [cell.strip("`* ").casefold() for cell in cells]
        if "condition" in headers or "conditions" in headers:
            condition_column = next(
                i
                for i, header in enumerate(headers)
                if header in {"condition", "conditions"}
            )
            continue
        if condition_column is None or all(
            re.fullmatch(r"[-: ]+", cell) for cell in cells
        ):
            continue
        if condition_column < len(cells):
            cell = cells[condition_column]
            names = (
                re.findall(r"`([a-z][a-z0-9_]*)`", cell)
                if re.fullmatch(CONDITION_LIST, cell)
                else []
            )
            if not names and CONDITION_NAME.fullmatch(cell):
                names = [cell]
            result.extend((index + 1, name) for name in names)
    start = 0
    while start < len(lines):
        if not lines[start].strip() or lines[start].lstrip().startswith(("|", "#")):
            start += 1
            continue
        end = start + 1
        while (
            end < len(lines)
            and lines[end].strip()
            and not lines[end].lstrip().startswith(("|", "#"))
        ):
            end += 1
        paragraph = "\n".join(lines[start:end])
        for pattern in (REFUSAL, NAMED_CONDITION):
            for match in pattern.finditer(paragraph):
                offset = match.start(1)
                for name in re.finditer(r"`([a-z][a-z0-9_]*)`", match[1]):
                    line = start + 1 + paragraph[: offset + name.start()].count("\n")
                    result.append((line, name[1]))
        start = end
    return sorted(set(result))


def rule_vocabulary_errors(root: Path, registered: dict[str, set[str]]) -> list[str]:
    errors = []
    for path in sorted((root / "rules").rglob("*.md")):
        for line, condition in rule_condition_references(
            path.read_text(encoding="utf-8")
        ):
            if condition not in registered:
                errors.append(
                    f"{path.relative_to(root).as_posix()}:{line}: "
                    f"rule names unregistered condition {condition}"
                )
    return errors


def decode_rust_string(source: str) -> str:
    """Decode ordinary Rust escapes without changing unescaped Unicode."""
    result = []
    offset = 0
    escapes = {
        "n": "\n",
        "r": "\r",
        "t": "\t",
        "0": "\0",
        '"': '"',
        "'": "'",
        "\\": "\\",
    }
    while offset < len(source):
        if source[offset] != "\\" or offset + 1 == len(source):
            result.append(source[offset])
            offset += 1
            continue
        offset += 1
        escaped = source[offset]
        if escaped in escapes:
            result.append(escapes[escaped])
            offset += 1
        elif escaped == "x" and re.fullmatch(
            r"[0-9a-fA-F]{2}", source[offset + 1 : offset + 3]
        ):
            result.append(chr(int(source[offset + 1 : offset + 3], 16)))
            offset += 3
        elif escaped == "u" and source[offset + 1 : offset + 2] == "{":
            end = source.find("}", offset + 2)
            if end < 0:
                result.append(escaped)
                offset += 1
                continue
            try:
                result.append(chr(int(source[offset + 2 : end].replace("_", ""), 16)))
            except ValueError:
                result.append(source[offset : end + 1])
            offset = end + 1
        elif escaped in "\r\n":
            while offset < len(source) and source[offset].isspace():
                offset += 1
        else:
            result.append(escaped)
            offset += 1
    return "".join(result)


def rust_literals(source: str):
    """Yield decoded string literals and their original first source line."""
    offset = 0
    line = 1
    while offset < len(source):
        if source.startswith("//", offset):
            end = source.find("\n", offset)
            offset = len(source) if end < 0 else end
            continue
        if source.startswith("/*", offset):
            depth = 1
            offset += 2
            while offset < len(source) and depth:
                if source.startswith("/*", offset):
                    depth += 1
                    offset += 2
                elif source.startswith("*/", offset):
                    depth -= 1
                    offset += 2
                else:
                    line += source[offset] == "\n"
                    offset += 1
            continue
        if source[offset] == "'":
            character = CHARACTER.match(source, offset)
            if character is not None:
                offset = character.end()
                continue
        raw = RAW_START.match(source, offset)
        if raw is not None and (
            offset == 0
            or not (source[offset - 1].isalnum() or source[offset - 1] == "_")
        ):
            delimiter = '"' + raw[1]
            end = source.find(delimiter, raw.end())
            if end < 0:
                return
            yield line, source[raw.end() : end]
            end += len(delimiter)
            line += source[offset:end].count("\n")
            offset = end
            continue
        if source[offset] == '"':
            end = offset + 1
            while end < len(source):
                if source[end] == "\\":
                    end += 2
                elif source[end] == '"':
                    break
                else:
                    end += 1
            if end >= len(source):
                return
            yield line, decode_rust_string(source[offset + 1 : end])
            line += source[offset : end + 1].count("\n")
            offset = end + 1
            continue
        line += source[offset] == "\n"
        offset += 1


def embedded_json_strings(source: str) -> list[str]:
    """Inspect JSON data, including format templates; ignore arbitrary prose."""
    try:
        value = json.loads(source)
    except (ValueError, RecursionError):
        if not source.lstrip().startswith(("{", "[")):
            return []
        result = []
        for literal in JSON_LITERAL.findall(source):
            try:
                result.append(json.loads(literal))
            except ValueError:
                continue
        value = result
    result = []
    pending = [value]
    while pending:
        value = pending.pop()
        if isinstance(value, str):
            result.append(value)
            try:
                nested = json.loads(value)
            except (ValueError, RecursionError):
                continue
            if isinstance(nested, (str, dict, list)):
                pending.append(nested)
        elif isinstance(value, dict):
            pending.extend(value.keys())
            pending.extend(value.values())
        elif isinstance(value, list):
            pending.extend(value)
    return result


def diagnostic_literals(
    source: str, registered: dict[str, set[str]], requirement: re.Pattern
) -> dict[int, list[str]]:
    """Keep exact Rust strings or JSON values with diagnostic spellings."""
    found = {}
    for line, literal in rust_literals(source):
        candidates = (
            [literal]
            if requirement.fullmatch(literal) or literal in registered
            else embedded_json_strings(literal)
        )
        selected = [
            value
            for value in candidates
            if requirement.fullmatch(value) or value in registered
        ]
        if selected:
            found.setdefault(line, []).extend(selected)
    return found


def private_unit_test_source(root: Path, relative: Path) -> bool:
    """Keep relocated independent assertions outside the production literal ratchet."""
    if relative != PRIVATE_TEST_MODULE:
        return False
    parent = root / relative.with_name("mod.rs")
    if not parent.is_file():
        return False
    source = parent.read_text(encoding="utf-8")
    declaration = "mod publication_failure_tests;"
    if source.count(declaration) != 1 or not re.search(
        r"(?m)^#\[cfg\(test\)\]\nmod publication_failure_tests;$", source
    ):
        return False
    # An explicit path or textual include must not make this file available to
    # production under another module name. Its only loader is the private cfg.
    for path in (root / "rust/crates").glob("*/src/**/*.rs"):
        if path != root / relative and re.search(
            r'"[^"\n]*publication_failure_tests\.rs"',
            path.read_text(encoding="utf-8"),
            re.IGNORECASE,
        ):
            return False
    return True


def vocabulary(root: Path) -> tuple[dict[str, set[str]], list[str]]:
    """Read the canonical inline registrations; the repository validator owns YAML."""
    path = root / "yaml/conditions.yaml"
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        return {}, [f"yaml/conditions.yaml: unavailable vocabulary: {error}"]
    registered = {}
    errors = []
    lines = text.splitlines()
    if "conditions:" not in lines:
        return {}, ["yaml/conditions.yaml: no condition registrations found"]
    for line in lines[lines.index("conditions:") + 1 :]:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        registration = REGISTRATION.fullmatch(line)
        if registration is None:
            errors.append(
                f"yaml/conditions.yaml: unrecognized registration: {line.strip()}"
            )
            continue
        condition, phases = registration.groups()
        if condition in registered:
            errors.append(f"yaml/conditions.yaml: duplicate registration: {condition}")
        registered[condition] = {phase.strip() for phase in phases.split(",")}
    if not registered:
        errors.append("yaml/conditions.yaml: no condition registrations found")
    return registered, errors


def null_conditions(
    root: Path, registered: dict[str, set[str]]
) -> tuple[set[tuple[str, str]], list[str]]:
    """Read the explicit normative exceptions, never infer absence from a cause."""
    try:
        source = (root / NULL_POLICY).read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        return set(), [f"{NULL_POLICY}: unavailable normative null policy: {error}"]
    if source.count(NULL_START) != 1 or source.count(NULL_END) != 1:
        return set(), [f"{NULL_POLICY}: expected one normative null policy block"]
    start = source.index(NULL_START) + len(NULL_START)
    end = source.index(NULL_END)
    if end < start:
        return set(), [f"{NULL_POLICY}: reversed normative null policy block"]
    exceptions = set()
    errors = []
    for line in source[start:end].splitlines():
        if not line.strip():
            continue
        entry = NULL_ENTRY.fullmatch(line)
        if entry is None:
            errors.append(f"{NULL_POLICY}: invalid normative null entry: {line}")
            continue
        condition, phase = entry.groups()
        if (condition, phase) in exceptions:
            errors.append(f"{NULL_POLICY}: duplicate normative null entry: {line}")
        exceptions.add((condition, phase))
        if phase not in registered.get(condition, set()):
            errors.append(f"{NULL_POLICY}: unregistered normative null entry: {line}")
    if not exceptions:
        errors.append(f"{NULL_POLICY}: no normative null conditions found")
    return exceptions, errors


def registry_errors(
    root: Path,
    registered: dict[str, set[str]],
    nullable: set[tuple[str, str]],
    cause_line: re.Pattern,
) -> list[str]:
    path = root / REGISTRY
    try:
        source = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        return [f"{path.relative_to(root)}: unavailable core registry: {error}"]
    block = re.search(r"^conditions! \{\n(.*?)^\}", source, re.MULTILINE | re.DOTALL)
    if block is None:
        return ["core diagnostic registry: no conditions found"]
    errors = []
    count = 0
    used_nulls = set()
    for line in block[1].splitlines():
        if not line.strip() or line.lstrip().startswith("//"):
            continue
        cause = cause_line.fullmatch(line)
        if cause is None:
            errors.append(
                f"core diagnostic registry: unrecognized cause: {line.strip()}"
            )
            continue
        count += 1
        name, phase, condition, requirement = cause.groups()
        if requirement == "None":
            used_nulls.add((condition, phase))
            if (condition, phase) not in nullable:
                errors.append(
                    f"core diagnostic registry:{name}: null requirement is not "
                    f"normatively permitted for {condition} at {phase}"
                )
        elif (condition, phase) in nullable:
            errors.append(
                f"core diagnostic registry:{name}: implementation condition "
                f"{condition} at {phase} must retain a null requirement"
            )
        if condition not in registered:
            errors.append(
                f"core diagnostic registry:{name}: unregistered condition {condition}"
            )
        elif phase not in registered[condition]:
            errors.append(
                f"core diagnostic registry:{name}: unregistered phase {phase} for {condition}"
            )
    if not count:
        errors.append("core diagnostic registry: no conditions found")
    for condition, phase in sorted(nullable - used_nulls):
        errors.append(
            f"{NULL_POLICY}: unused normative null condition {condition} at {phase}"
        )
    return errors


def violations(root: Path) -> list[str]:
    """Check every mapping and exact existing literals outside the canonical registry."""
    rules = load(root)
    defined = rules.identifiers()
    if not defined:
        return ["no rule definitions found"]
    sources = sorted((root / "rust/crates").glob("*/src/**/*.rs"))
    if not sources:
        return ["no Rust crate sources found"]
    registered, errors = vocabulary(root)
    nullable, null_errors = null_conditions(root, registered)
    errors.extend(null_errors)
    errors.extend(registry_errors(root, registered, nullable, cause_pattern(rules)))
    errors.extend(rule_vocabulary_errors(root, registered))
    requirement = rules.identifier
    for path in sources:
        relative = path.relative_to(root).as_posix()
        crate = path.relative_to(root / "rust/crates").parts[0]
        source = path.read_text(encoding="utf-8")
        test_only = private_unit_test_source(root, Path(relative))
        for number, literals in diagnostic_literals(
            source, registered, requirement
        ).items():
            for identifier in literals:
                if requirement.fullmatch(identifier) and identifier not in defined:
                    errors.append(
                        f"{relative}:{number}: undefined requirement {identifier}"
                    )
            if (
                Path(relative) in (REGISTRY, SHIPPED_SCHEMA)
                or test_only
                or not literals
            ):
                continue
            errors.append(
                f"{relative}:{number}: new diagnostic literal site "
                + (
                    "outside canonical registry: "
                    if crate in CORE_CRATES
                    else "outside core: "
                )
                + ", ".join(literals)
            )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    arguments = parser.parse_args()
    errors = violations(arguments.root)
    for error in errors:
        print(error)
    if errors:
        return 1
    print("PASS: diagnostic mappings are registered and literals stay in the registry.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
