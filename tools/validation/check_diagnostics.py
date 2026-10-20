"""Guard normative mappings and keep diagnostic literals in their registry.

Core registry conditions and phases must be in the shared vocabulary. Rule error
tables/refusal clauses must name registered conditions. A cause's citation names
a card, directly or as a REQ ID the card's Replaces line lists, and that card's
Fails line names the cause's condition, so one fault keeps the one condition its
card states. A legacy requirement block has no Fails line and cannot be cited. A
registered condition needs no core cause or separate disposition record; the
vocabulary and the core registry are the only sources. These are static
ownership checks, not a claim that every registered condition has executable
release coverage.
Condition and requirement literals outside the canonical registry, the generated
shipped schema and private unit tests are refused. A requirement is a legacy
REQ ID or a card ID, read from the shared rules model.
A condition written as a literal must also name a registered condition: the
value of a `condition` field, key or binding, in Rust or in JSON text, outside
the items that compile only for tests. Matching registered names alone would
miss an issue row built with a condition the registry does not have.
A diagnostic record crosses Rust modules typed, so JSON text is decoded only
in a codec that owns its format. Wire helpers and serde JSON readers, including
imports and field deserializers, are refused elsewhere outside test-only code.
"""

import argparse
import bisect
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
RUST_NAME = r"(?:r#)?[A-Za-z_][A-Za-z_0-9]*"
RUST_PATH = re.compile(
    rf"(?<![A-Za-z_0-9#])(?:::\s*)?{RUST_NAME}(?:\s*::\s*{RUST_NAME})*"
)
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
    """JSON reading capabilities outside test items, including Rust imports.

    Resolve use trees before checking paths so renames, nested imports and
    module aliases retain their owner's identity. Importing a reader or a
    glob that exposes readers is itself a site, as in the original guard.
    Serialization, typed tree projection and ordinary string literals are
    not decoding. Serde's field-deserializer string is executable metadata,
    so inspect that string only inside its actual serde attribute.
    """
    tokens = list(rust_tokens(source))
    code = blank(source, tokens, {"comment", "character", "string"})
    tests = cfg_test_items(code)
    code = blank(code, [("test", low, high, 0, None) for low, high in tests], {"test"})
    imports = RustImports(code)

    def reader(path, offset):
        return any(
            reader_path(candidate)
            for candidate in imports.resolve(path, imports.scope_at(offset))
            if isinstance(candidate, tuple)
        )

    def reader_path(path):
        for owner in ("json_wire", "serde_json"):
            if owner not in path:
                continue
            tail = path[path.index(owner) + 1 :]
            if owner == "json_wire":
                return bool(tail) and (
                    tail[0] == "*"
                    or tail[0].startswith("decode")
                    or tail[0] in {"deserialize_value", "WireValueOwned"}
                )
            return bool(tail) and (
                tail in {("*",), ("de", "*")}
                or any(
                    part in {"from_str", "from_slice", "from_reader", "Deserializer"}
                    for part in tail
                )
                or (tail[-1] == "deserialize" and "Value" in tail)
            )
        return False

    sites = {
        offset
        for path, _, offset, scope in imports.leaves
        if any(
            reader_path(candidate)
            for candidate in imports.resolve(path, scope)
            if isinstance(candidate, tuple)
        )
    }
    for match in RUST_PATH.finditer(imports.code_without_uses):
        if reader(tuple(re.split(r"\s*::\s*", match[0])), match.start()):
            sites.add(match.start())
    # A qualified trait call separates the receiver from `deserialize`:
    # `<Value as serde::Deserialize>::deserialize(deserializer)`.
    for match in re.finditer(
        rf"<\s*({RUST_PATH.pattern})\s+as\s+{RUST_PATH.pattern}(?:\s*<[^>]*>)?\s*>\s*::\s*deserialize\b",
        code,
    ):
        if reader(
            tuple(re.split(r"\s*::\s*", match[1])) + ("deserialize",), match.start()
        ):
            sites.add(match.start())
    for attribute in re.finditer(r"#\s*\[\s*serde\s*\(", code):
        end = balanced(code, code.index("[", attribute.start()), "[", "]")
        if end is None:
            continue
        for kind, start, _, _, value in tokens:
            if (
                kind == "string"
                and attribute.end() <= start < end
                and re.search(
                    r"\bdeserialize_with\s*=\s*$", code[attribute.end() : start]
                )
                and reader(tuple(re.split(r"\s*::\s*", value.strip())), start)
            ):
                sites.add(start)
    return sorted({source.count("\n", 0, offset) + 1 for offset in sites})


def rust_imports(
    tree: str, offset: int = 0
) -> list[tuple[tuple[str, ...], str | None, int]]:
    """Expand a Rust use tree into (path, local name, source offset) leaves."""
    tokens = list(re.finditer(rf"{RUST_NAME}|::|[{{}},*]", tree))
    result = []
    index = 0

    def branch(prefix):
        nonlocal index
        path = prefix
        start = tokens[index].start() if index < len(tokens) else 0
        alias = None
        while index < len(tokens):
            token = tokens[index][0]
            index += 1
            if token == "::":
                if not path:
                    path = ("",)
                continue
            if token == "{":
                while index < len(tokens) and tokens[index][0] != "}":
                    branch(path)
                    if index < len(tokens) and tokens[index][0] == ",":
                        index += 1
                index += 1
                return
            if token == "as":
                if index < len(tokens):
                    alias = tokens[index][0]
                    index += 1
                break
            if token in {",", "}"}:
                index -= 1
                break
            if token != "self" or not path:
                path += (token,)
        if path:
            result.append(
                (path, alias or (None if path[-1] == "*" else path[-1]), offset + start)
            )

    branch(())
    return result


class RustImports:
    """Resolve use items within their Rust block and module scopes.

    Items apply throughout their block. Blocks inherit imports; child modules
    do not, and `super` skips enclosing function blocks. Each alias resolves
    in its declaration scope, so a caller's shadow cannot retarget it. Multiple
    declarations in one scope are kept for conditional compilation alternatives.
    This resolves source ownership, not Rust's privacy or type checking.
    """

    def __init__(self, code: str):
        uses = list(re.finditer(r"\buse\s+([^;]+);", code))
        self.code_without_uses = blank(
            code,
            [("use", match.start(), match.end(), 0, None) for match in uses],
            {"use"},
        )
        # Import-tree braces are not lexical blocks.
        modules = {
            match.end() - 1: match[1].removeprefix("r#")
            for match in re.finditer(
                rf"\bmod\s+({RUST_NAME})\s*\{{", self.code_without_uses
            )
        }
        self.starts = [0]
        self.ends = [len(code)]
        self.parents = [None]
        self.module = [0]
        self.module_paths = [("crate",)]
        self.bindings = [{}]
        self.globs = [[]]
        stack = [0]
        for match in re.finditer(r"[{}]", self.code_without_uses):
            if match[0] == "}":
                if len(stack) > 1:
                    self.ends[stack.pop()] = match.end()
                continue
            parent = stack[-1]
            scope = len(self.starts)
            self.starts.append(match.start())
            self.ends.append(len(code))
            self.parents.append(parent)
            name = modules.get(match.start())
            self.module.append(scope if name is not None else self.module[parent])
            self.module_paths.append(
                self.module_paths[self.module[parent]] + (name,)
                if name is not None
                else self.module_paths[self.module[parent]]
            )
            self.bindings.append({})
            self.globs.append([])
            if name is not None:
                self.bindings[parent].setdefault(name, []).append(
                    (scope, parent, match.start())
                )
            stack.append(scope)
        self.leaves = []
        for match in uses:
            scope = self.scope_at(match.start())
            for path, alias, offset in rust_imports(match[1], match.start(1)):
                path = tuple(part.removeprefix("r#") for part in path)
                self.leaves.append((path, alias, offset, scope))
                binding = (path, scope, offset)
                if alias is None:
                    self.globs[scope].append(binding)
                else:
                    self.bindings[scope].setdefault(
                        alias.removeprefix("r#"), []
                    ).append(binding)

    def scope_at(self, offset):
        scope = bisect.bisect_right(self.starts, offset) - 1
        while offset >= self.ends[scope] and self.parents[scope] is not None:
            scope = self.parents[scope]
        return scope

    def resolve(self, path, scope, seen=frozenset()):
        path = tuple(part.removeprefix("r#") for part in path)
        if not path:
            return set()
        if path[0] == "":  # Absolute paths name the extern prelude.
            return {path}
        if path[0] in {"crate", "self", "super"}:
            target = 0 if path[0] == "crate" else self.module[scope]
            if path[0] == "super":
                parent = self.parents[target]
                target = self.module[parent] if parent is not None else 0
            path = path[1:]
            while path and path[0] == "super":
                parent = self.parents[target]
                target = self.module[parent] if parent is not None else 0
                path = path[1:]
            targets = {target}
        else:
            targets = self.lookup(path[0], scope, seen, lexical=True)
            if not targets:
                targets = {(path[0],)}
            path = path[1:]
        for name in path:
            expanded = set()
            for target in targets:
                if isinstance(target, tuple):
                    expanded.add(target + (name,))
                else:
                    members = self.lookup(name, target, seen, lexical=False)
                    expanded.update(members or {self.module_paths[target] + (name,)})
            targets = expanded
        return targets

    def lookup(self, name, scope, seen, *, lexical):
        while True:
            bindings = self.bindings[scope].get(name)
            if bindings:
                targets = set()
                for path, declaration, offset in bindings:
                    key = (declaration, offset)
                    if key in seen:
                        continue
                    if isinstance(path, int):
                        targets.add(path)
                    else:
                        targets.update(self.resolve(path, declaration, seen | {key}))
                return targets
            targets = set()
            for path, declaration, offset in self.globs[scope]:
                key = (declaration, offset)
                if key in seen:
                    continue
                for owner in self.resolve(path[:-1], declaration, seen | {key}):
                    if isinstance(owner, int):
                        targets.update(
                            self.lookup(name, owner, seen | {key}, lexical=False)
                        )
                    elif owner[-2:] == ("serde_json", "value") and name in {
                        "Value",
                        "Map",
                        "Number",
                        "Serializer",
                        "from_value",
                        "to_value",
                    }:
                        # The value module exports trees and projections, not
                        # text readers. Resolve its types only when used, so
                        # Value::deserialize is caught without banning trees.
                        targets.add(owner + (name,))
            if targets:
                return targets
            if (
                not lexical
                or self.module[scope] == scope
                or self.parents[scope] is None
            ):
                return set()
            scope = self.parents[scope]


def json_test_sources(sources: dict[Path, str]) -> set[Path]:
    """External modules whose every code loader is test-only.

    A filename such as `tests.rs` grants no exemption. Follow ordinary module
    declarations, explicit path attributes and literal includes; a production
    alias of a test module prevents exemption. Children of a proven test
    module are test-only too. Unresolved includes conservatively prevent this
    exemption, since their code target cannot be proved here.
    """
    incoming: dict[Path, list[tuple[Path, bool]]] = {}
    unresolved = []
    for path, source in sources.items():
        tokens = list(rust_tokens(source))
        code = blank(source, tokens, {"comment", "character", "string"})
        visible = blank(source, tokens, {"comment"})
        tests = cfg_test_items(code)

        def gated(offset):
            return any(low <= offset < high for low, high in tests)

        inline = []
        for match in re.finditer(rf"\bmod\s+({RUST_NAME})\s*\{{", code):
            end = balanced(code, match.end() - 1, "{", "}")
            if end is not None:
                inline.append((match.start(), end, match[1]))
        for match in re.finditer(
            rf"((?:#\s*\[[^\]]*\]\s*)*)(?:pub(?:\s*\([^)]*\))?\s+)?"
            rf"\bmod\s+({RUST_NAME})\s*;",
            visible,
        ):
            # Comments are already blank; reject matches inside string literals.
            start = match.end() - len(match[0].lstrip())
            if not re.search(r"\bmod\s+", code[match.start() : match.end()]):
                continue
            directories = [name for low, high, name in inline if low < start < high]
            explicit = [
                text
                for kind, start, _, _, text in rust_tokens(match[1])
                if kind == "string" and re.search(r"\bpath\s*=\s*$", match[1][:start])
            ]
            explicit_base = path.parent
            # Inside an inline module, Rust includes a non-mod-rs file's
            # stem in relative paths. A top-level #[path] remains relative
            # to the file's own directory.
            if directories and path.stem not in {"lib", "main", "mod"}:
                explicit_base /= path.stem
            candidates = [
                explicit_base.joinpath(*directories) / name for name in explicit
            ]
            # A conditional path can still load the ordinary module on the
            # other configuration. Both loaders must be accounted for.
            if not re.search(r"#\s*\[\s*path\s*=", match[1]):
                base = path.parent
                if path.stem not in {"lib", "main", "mod"}:
                    base /= path.stem
                base = base.joinpath(*directories)
                candidates.extend([base / f"{match[2]}.rs", base / match[2] / "mod.rs"])
            for candidate in candidates:
                target = candidate.resolve()
                if target in sources:
                    incoming.setdefault(target, []).append((path, gated(start)))
        for match in re.finditer(r"\binclude\s*!\s*\(", code):
            end = balanced(code, match.end() - 1, "(", ")")
            strings = [
                text
                for kind, start, stop, _, text in tokens
                if kind == "string" and match.end() <= start < stop <= (end or 0)
            ]
            body = code[match.end() : (end or match.end()) - 1].strip()
            if end is None or len(strings) != 1 or body:
                unresolved.append((path, gated(match.start())))
            else:
                target = (path.parent / strings[0]).resolve()
                if target in sources:
                    incoming.setdefault(target, []).append((path, gated(match.start())))
    result = set()
    while True:
        added = {
            path
            for path, loaders in incoming.items()
            if path.stem not in {"lib", "main"}
            and all(test or parent in result for parent, test in loaders)
        } - result
        if not added:
            break
        result.update(added)
    if any(not test and parent not in result for parent, test in unresolved):
        return set()
    return result


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


def citation_error(name: str, condition: str, cited: str, rules) -> str | None:
    """Why a cause's citation names no card stating its condition, or None.

    A card ID names its card and a REQ ID names the card whose Replaces line
    lists it; that card's Fails line must name the condition. An ID that names
    nothing is reported where literals are checked, as an undefined requirement.
    """
    card = rules.cards.get(cited) or rules.replaced.get(cited)
    if card is None:
        if cited not in rules.requirements:
            return None
        return (
            f"core diagnostic registry:{name}: {cited} is a legacy requirement "
            f"with no Fails line naming {condition}"
        )
    if condition in card.fails:
        return None
    if card.id == cited:
        return (
            f"core diagnostic registry:{name}: {cited} does not name "
            f"{condition} in its Fails line"
        )
    return (
        f"core diagnostic registry:{name}: {cited} is replaced by {card.id}, "
        f"which does not name {condition} in its Fails line"
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
        else:
            cited = requirement.removeprefix('Some("').removesuffix('")')
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
    root = root.resolve()
    rules = load(root)
    defined = rules.identifiers()
    if not defined:
        return ["no rule definitions found"]
    sources = {
        path.resolve(): path.read_text(encoding="utf-8")
        for path in sorted((root / "rust/crates").glob("*/src/**/*.rs"))
    }
    if not sources:
        return ["no Rust crate sources found"]
    registered, errors = vocabulary(root)
    nullable, null_errors = null_conditions(root, registered)
    errors.extend(null_errors)
    errors.extend(
        registry_errors(root, registered, nullable, cause_pattern(rules), rules)
    )
    errors.extend(rule_vocabulary_errors(root, registered))
    requirement = rules.identifier
    json_tests = json_test_sources(sources)
    for path, source in sources.items():
        relative = path.relative_to(root).as_posix()
        crate = path.relative_to(root / "rust/crates").parts[0]
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
        if Path(relative) not in JSON_CODECS and path not in json_tests:
            for number in json_decodes(source):
                errors.append(f"{relative}:{number}: JSON text decoded outside a codec")
        if Path(relative) in (REGISTRY, SHIPPED_SCHEMA) or test_only:
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
