"""Guard normative mappings and keep diagnostic literals in their registry.

Core registry conditions and phases must be in the shared vocabulary. Rule error
tables/refusal clauses must name registered conditions. A cause's citation names
a card whose Fails line names the cause's condition, so one fault keeps the one
condition its card states. A registered condition needs no core cause or
separate disposition record; the vocabulary and the core registry are the only
sources. These are static ownership checks, not a claim that every registered
condition has executable release coverage.
Condition and rule literals outside the canonical registry, the generated
shipped schema and private unit tests are refused. A rule is a card ID, read
from the shared rules model.
A condition written as a literal must also name a registered condition: the
value of a `condition` field, key or binding, in Rust or in JSON text, outside
the items that compile only for tests. Matching registered names alone would
miss an issue row built with a condition the registry does not have.
A diagnostic record crosses Rust modules typed, so JSON text is decoded only
in a codec that owns its format: a `json_wire` decoder anywhere else, outside
the items that compile only for tests, is refused, whether it is named by path,
through a grouped, aliased or glob import, or as a serde route (the owned wire
value a serde decoder fills, or a `deserialize_with` hook). A module file that
a test gate declares out of line compiles only for tests as a whole.
"""

import argparse
import json
import os
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
NULL_POLICY = Path("rules/columns.md")
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
# The code just before a string literal that makes the literal a condition: a
# `condition` field or JSON key, optionally through a conversion call, or a
# `condition` binding, constant or assignment.
CONDITION_SITE = re.compile(
    r'(?:(?:\bcondition|"condition")\s*:\s*'
    r"(?:(?:String::from|str::to_owned|ToOwned::to_owned|ToString::to_string"
    r"|Into::into|Cow::from|Cow::Borrowed)\s*\(\s*)?"
    r"|(?i:\bcondition)\s*(?::[^=;{}()]*)?=\s*)\Z"
)
CONDITION_KEY = re.compile(r'"condition"\s*:\s*("(?:[^"\\]|\\.)*")')
# The `json_wire` items that read JSON text: its decoders, the serde hook that
# deserializes a field and the owned wire value a serde decoder fills.
JSON_DECODER = r"(?:decode\w*|deserialize_value|WireValueOwned)\b"
# A local name for the `json_wire` module: `json_wire as name` or a grouped
# `self as name`.
JSON_MODULE_ALIAS = re.compile(
    r"\bjson_wire\s+as\s+(\w+)|\bjson_wire\s*::\s*\{[^}]*\bself\s+as\s+(\w+)"
)
# The code just before a string attribute value that names a serde function.
SERDE_HOOK = re.compile(r"\b(?:deserialize_with|with)\s*=\s*\Z")
# An out-of-line module declaration, after any attributes and visibility.
MODULE_FILE = re.compile(r"\bmod\s+(\w+)\s*;")
VISIBILITY = re.compile(r"\bpub(?:\s*\([^)]*\))?\s*\Z")
PATH_ATTRIBUTE = re.compile(r'#\[\s*path\s*=\s*"([^"]*)"\s*\]\Z')
# The codecs that read JSON text they own: the wire module itself, the
# package-lock reader, the stored report rows and the resolved artifact.
JSON_CODECS = {
    Path("rust/crates/yamaa-adapters/src/json_wire.rs"),
    Path("rust/crates/yamaa-adapters/src/project_lock.rs"),
    Path("rust/crates/yamaa-adapters/src/report_storage.rs"),
    Path("rust/crates/yamaa-adapters/src/resolved_artifact/model.rs"),
}
SPACE = re.compile(r"\s*")


def cause_pattern(rules) -> re.Pattern:
    """One registry cause; its rule is a card ID, or None."""
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


def rust_tokens(source: str):
    """Yield every comment, character and string literal of Rust source.

    A token is ``(kind, start, end, line, text)``: ``kind`` is ``comment``,
    ``character`` or ``string``, ``source[start:end]`` is the whole token,
    ``line`` is its first source line and ``text`` is a string's decoded value
    (None otherwise). An unterminated string ends the scan.
    """
    offset = 0
    line = 1
    while offset < len(source):
        if source.startswith("//", offset):
            end = source.find("\n", offset)
            end = len(source) if end < 0 else end
            yield "comment", offset, end, line, None
            offset = end
            continue
        if source.startswith("/*", offset):
            start, first = offset, line
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
            yield "comment", start, offset, first, None
            continue
        if source[offset] == "'":
            character = CHARACTER.match(source, offset)
            if character is not None:
                yield "character", offset, character.end(), line, None
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
            yield "string", offset, end + len(delimiter), line, source[raw.end() : end]
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
            yield (
                "string",
                offset,
                end + 1,
                line,
                decode_rust_string(source[offset + 1 : end]),
            )
            line += source[offset : end + 1].count("\n")
            offset = end + 1
            continue
        line += source[offset] == "\n"
        offset += 1


def rust_literals(source: str):
    """Yield decoded string literals and their original first source line."""
    for kind, _, _, line, text in rust_tokens(source):
        if kind == "string":
            yield line, text


def blank(source: str, tokens: list, kinds: set[str]) -> str:
    """The source with every token of ``kinds`` spaced out, lines kept."""
    pieces = []
    offset = 0
    for kind, start, end, _, _ in tokens:
        if kind in kinds:
            pieces.append(source[offset:start])
            pieces.append(re.sub(r"[^\n]", " ", source[start:end]))
            offset = end
    pieces.append(source[offset:])
    return "".join(pieces)


def balanced(code: str, start: int, opening: str, closing: str) -> int | None:
    """The offset just past the bracket that closes the one at ``start``."""
    depth = 0
    for index in range(start, len(code)):
        if code[index] == opening:
            depth += 1
        elif code[index] == closing:
            depth -= 1
            if not depth:
                return index + 1
    return None


def is_test_gate(attribute: str) -> bool:
    """Whether one attribute compiles its item for tests only: exactly
    ``#[cfg(test)]``, or ``test`` as a top-level operand of ``#[cfg(all(...))]``.
    ``#[cfg(not(test))]`` and feature names that contain ``test`` do not count.
    """
    body = re.sub(r"\s+", "", attribute)
    if body == "#[cfg(test)]":
        return True
    operands = re.fullmatch(r"#\[cfg\(all\((.*)\)\)\]", body)
    if operands is None:
        return False
    depth = 0
    operand = ""
    for character in operands[1] + ",":
        if character == "," and not depth:
            if operand == "test":
                return True
            operand = ""
            continue
        depth += (character == "(") - (character == ")")
        operand += character
    return False


def cfg_test_items(code: str) -> list[tuple[int, int]]:
    """The offsets of every item that a test gate compiles for tests only.

    ``code`` is Rust source with its comments and literals spaced out, so only
    real brackets count. A gated item follows its attributes and ends at its
    closing brace or, without a body, at its semicolon.
    """
    regions = []
    offset = 0
    while (start := code.find("#[", offset)) >= 0:
        end = balanced(code, start + 1, "[", "]")
        if end is None:
            break
        offset = end
        if not is_test_gate(code[start:end]):
            continue
        item = SPACE.match(code, end).end()
        while code.startswith("#[", item):
            item = SPACE.match(
                code, balanced(code, item + 1, "[", "]") or len(code)
            ).end()
        depth = 0
        stop = len(code)
        for index in range(item, len(code)):
            character = code[index]
            if character in "([":
                depth += 1
            elif character in ")]":
                depth -= 1
            elif character == ";" and not depth:
                stop = index + 1
                break
            elif character == "{" and not depth:
                stop = balanced(code, index, "{", "}") or len(code)
                break
        regions.append((start, stop))
        offset = stop
    return regions


def json_conditions(text: str) -> list[str]:
    """The values of every ``condition`` key in JSON text, nested JSON
    strings and format templates included."""
    try:
        pending = [json.loads(text)]
    except (ValueError, RecursionError):
        if not text.lstrip().startswith(("{", "[")):
            return []
        found = []
        for literal in CONDITION_KEY.findall(text):
            try:
                found.append(json.loads(literal))
            except ValueError:
                continue
        return found
    found = []
    while pending:
        value = pending.pop()
        if isinstance(value, dict):
            condition = value.get("condition")
            if isinstance(condition, str):
                found.append(condition)
            pending.extend(value.values())
        elif isinstance(value, list):
            pending.extend(value)
        elif isinstance(value, str):
            try:
                nested = json.loads(value)
            except (ValueError, RecursionError):
                continue
            if isinstance(nested, (dict, list)):
                pending.append(nested)
    return found


def condition_literals(source: str) -> list[tuple[int, str]]:
    """Every condition written as a literal, with its first source line.

    A string literal is a condition when the code just before it is a
    ``condition`` field, key, binding or assignment (comments ignored), and a
    literal holding JSON text names one under each ``condition`` key. Items
    that compile only for tests hold fixtures, not issue rows.
    """
    tokens = list(rust_tokens(source))
    tests = cfg_test_items(blank(source, tokens, {"comment", "character", "string"}))
    code = blank(source, tokens, {"comment"})
    found = []
    for kind, start, _, line, text in tokens:
        if kind != "string" or any(low <= start < high for low, high in tests):
            continue
        if CONDITION_SITE.search(code[max(0, start - 120) : start]):
            found.append((line, text))
        found.extend((line, condition) for condition in json_conditions(text))
    return found


def json_decodes(source: str) -> list[int]:
    """The lines that decode JSON text through `json_wire`, outside items that
    compile only for tests: a decoder named by path or a grouped import under
    the module or a local alias of it, a decoder named bare where the module
    is glob imported, and a serde hook naming one. Comments never count, and
    other literals count only as a serde hook's value."""
    tokens = list(rust_tokens(source))
    code = blank(source, tokens, {"comment", "character", "string"})
    tests = cfg_test_items(code)
    modules = {"json_wire"}
    for match in JSON_MODULE_ALIAS.finditer(code):
        modules.update(alias for alias in match.groups() if alias)
    module = "(?:" + "|".join(sorted(map(re.escape, modules))) + ")"
    routes = [
        re.compile(rf"\b{module}\s*::\s*(?:{JSON_DECODER}|\{{[^}}]*\b{JSON_DECODER})")
    ]
    if re.search(rf"\b{module}\s*::\s*\*", code):
        routes.append(re.compile(rf"(?<![\w:.]){JSON_DECODER}"))
    offsets = [match.start() for route in routes for match in route.finditer(code)]
    hook = re.compile(rf"(?:\A|::){module}(?:::{JSON_DECODER})?\Z")
    commented = blank(source, tokens, {"comment"})
    offsets.extend(
        start
        for kind, start, _, _, text in tokens
        if kind == "string"
        and SERDE_HOOK.search(commented[max(0, start - 40) : start])
        and hook.search(text)
    )
    return sorted(
        {
            code.count("\n", 0, offset) + 1
            for offset in offsets
            if not any(low <= offset < high for low, high in tests)
        }
    )


def module_files(source: str) -> list[tuple[str, str | None, bool]]:
    """Every module the source declares out of line: its name, the file a
    ``#[path]`` attribute names (None for the default location) and whether a
    test gate among its attributes compiles it for tests only."""
    tokens = list(rust_tokens(source))
    code = blank(source, tokens, {"comment", "character", "string"})
    kept = blank(source, tokens, {"comment"})
    found = []
    for match in MODULE_FILE.finditer(code):
        start = match.start()
        visibility = VISIBILITY.search(code, 0, start)
        if visibility is not None:
            start = visibility.start()
        attributes = []
        while (end := len(code[:start].rstrip())) and code[end - 1] == "]":
            depth = 0
            for opening in range(end - 1, -1, -1):
                depth += (code[opening] == "]") - (code[opening] == "[")
                if not depth:
                    break
            if depth or opening == 0 or code[opening - 1] != "#":
                break
            attributes.append(kept[opening - 1 : end])
            start = opening - 1
        path = next(
            (
                attribute[1]
                for text in attributes
                if (attribute := PATH_ATTRIBUTE.fullmatch(text)) is not None
            ),
            None,
        )
        found.append((match[1], path, any(map(is_test_gate, attributes))))
    return found


def test_module_files(sources: list[Path]) -> set[Path]:
    """The module files that compile only for tests: each declared out of line
    under a test gate, or by such a file, at the default location for its
    declaring file or at the file its ``#[path]`` attribute names."""
    children = {}
    for path in sources:
        for name, written, gated in module_files(path.read_text(encoding="utf-8")):
            if written is not None:
                candidates = [path.parent / written]
            else:
                base = (
                    path.parent
                    if path.name in {"lib.rs", "main.rs", "mod.rs"}
                    else path.with_suffix("")
                )
                candidates = [base / f"{name}.rs", base / name / "mod.rs"]
            children.setdefault(path, []).append(
                ([Path(os.path.normpath(each)) for each in candidates], gated)
            )
    tests = set()
    pending = list(sources)
    while pending:
        parent = pending.pop()
        for candidates, gated in children.get(parent, []):
            if not (gated or parent in tests):
                continue
            for candidate in candidates:
                if candidate.is_file() and candidate not in tests:
                    tests.add(candidate)
                    pending.append(candidate)
    return tests


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
    source: str, registered: dict[str, set[str]], rule: re.Pattern
) -> dict[int, list[str]]:
    """Keep exact Rust strings or JSON values with diagnostic spellings."""
    found = {}
    for line, literal in rust_literals(source):
        candidates = (
            [literal]
            if rule.fullmatch(literal) or literal in registered
            else embedded_json_strings(literal)
        )
        selected = [
            value
            for value in candidates
            if rule.fullmatch(value) or value in registered
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


def citation_error(name: str, condition: str, cited: str, rules) -> str | None:
    """Why a cause's citation names no card stating its condition, or None.

    The cited card's Fails line must name the condition. An ID that names
    nothing is reported where literals are checked, as an undefined rule.
    """
    card = rules.cards.get(cited)
    if card is None or condition in card.fails:
        return None
    return (
        f"core diagnostic registry:{name}: {cited} does not name "
        f"{condition} in its Fails line"
    )


def registry_errors(
    root: Path,
    registered: dict[str, set[str]],
    nullable: set[tuple[str, str]],
    cause_line: re.Pattern,
    rules,
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
        name, phase, condition, rule = cause.groups()
        if rule == "None":
            used_nulls.add((condition, phase))
            if (condition, phase) not in nullable:
                errors.append(
                    f"core diagnostic registry:{name}: null rule is not "
                    f"normatively permitted for {condition} at {phase}"
                )
        elif (condition, phase) in nullable:
            errors.append(
                f"core diagnostic registry:{name}: implementation condition "
                f"{condition} at {phase} must retain a null rule"
            )
        else:
            cited = rule.removeprefix('Some("').removesuffix('")')
            error = citation_error(name, condition, cited, rules)
            if error is not None:
                errors.append(error)
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
    errors.extend(
        registry_errors(root, registered, nullable, cause_pattern(rules), rules)
    )
    errors.extend(rule_vocabulary_errors(root, registered))
    rule = rules.identifier
    test_files = test_module_files(sources)
    for path in sources:
        relative = path.relative_to(root).as_posix()
        crate = path.relative_to(root / "rust/crates").parts[0]
        source = path.read_text(encoding="utf-8")
        test_only = private_unit_test_source(root, Path(relative))
        for number, literals in diagnostic_literals(source, registered, rule).items():
            for identifier in literals:
                if rule.fullmatch(identifier) and identifier not in defined:
                    errors.append(f"{relative}:{number}: undefined rule {identifier}")
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
        if Path(relative) not in JSON_CODECS and path not in test_files:
            for number in json_decodes(source):
                errors.append(f"{relative}:{number}: JSON text decoded outside a codec")
        if (
            Path(relative) in (REGISTRY, SHIPPED_SCHEMA)
            or test_only
            or path in test_files
        ):
            continue
        # A registered literal is already a new site above; an unregistered
        # one names a condition that no registry, card or host knows.
        for number, condition in condition_literals(source):
            if condition not in registered:
                errors.append(
                    f"{relative}:{number}: unregistered condition literal {condition}"
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
