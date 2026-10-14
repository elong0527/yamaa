"""Guard normative mappings and ratchet diagnostic literals into their registry.

Core registry conditions and phases must be in the shared vocabulary. Rule error
tables/refusal clauses must name registered conditions; vocabulary without a core
cause needs a finite, source-owned disposition. These are static ownership checks,
not a claim that every registered condition has executable release coverage. The
frozen seed records the original literal sites; the active allowlist may only shrink.
Sites match a path and complete source line, with an occurrence count, rather
than a condition spelling or current line number. Source line numbers in seed
keys identify the original sites and survive unrelated insertions above them.
"""

import argparse
import ast
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

DEFINITION = re.compile(r"^\*\*(REQ-[0-9]{4,})\.\*\*", re.MULTILINE)
REQUIREMENT = re.compile(r"REQ-[0-9]{4,}\Z")
JSON_LITERAL = re.compile(r'"(?:[^"\\]|\\.)*"')
RAW_START = re.compile(r'(?:br|cr|r)(#*)"')
CHARACTER = re.compile(r"'(?:[^'\\\r\n]|\\(?:u\{[0-9a-fA-F_]+\}|[^\r\n]))'")
REGISTRATION = re.compile(
    r"^  ([a-z][a-z0-9_]*): \{phases: \[([a-z_, ]+)\]\}$", re.MULTILINE
)
CAUSE = re.compile(
    r'\s*(\w+) => \("([a-z_]+)", "([a-z_]+)", '
    r'(Some\("REQ-[0-9]{4,}"\)|None)\),\s*'
)
HOST_CRATES = {"yamaa-adapters", "yamaa-python", "yamaa-r"}
CORE_CRATES = {"yamaa-core", "yamaa-engine"}
SEED = Path("rust/tools/diagnostic_literal_seed.json")
ALLOWLIST = Path("rust/tools/diagnostic_literal_allowlist.json")
CORE_SEED = Path("rust/tools/core_diagnostic_literal_seed.json")
CORE_ALLOWLIST = Path("rust/tools/core_diagnostic_literal_allowlist.json")
REGISTRY = Path("rust/crates/yamaa-core/src/diagnostic/mod.rs")
SHIPPED_SCHEMA = Path("rust/crates/yamaa-adapters/src/shipped_schema/data.rs")
PRIVATE_TEST_MODULE = Path(
    "rust/crates/yamaa-adapters/src/file_define/publication_failure_tests.rs"
)
CHECKER = Path("rust/tools/check_diagnostics.py")
NULL_POLICY = Path("rules/execution/lifecycle.md")
NULL_START = "<!-- normative-null-conditions:start -->"
NULL_END = "<!-- normative-null-conditions:end -->"
NULL_ENTRY = re.compile(r"- `([a-z][a-z0-9_]*)` \(`([a-z_]+)`\)")
DISPOSITIONS = Path("rust/tools/diagnostic_vocabulary_dispositions.json")
DISPOSITION_KINDS = {
    "reserved",
    "reference_invariant",
    "reference_route",
    "historical_policy",
    "validator",
    "capability",
}
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


def rule_condition_references(source: str) -> list[tuple[int, str]]:
    """Read actual condition columns and refusal prose, not arbitrary code spans.

    Fenced examples and HTML comments do not declare vocabulary. Refusal clauses
    are joined across Markdown paragraph wraps, including comma/or lists. A
    condition column explicitly declares its cells regardless of surrounding
    headings; field/operation columns and ordinary inline code remain opaque.
    """
    source = re.sub(
        r"<!--.*?-->",
        lambda match: "\n" * match[0].count("\n"),
        source,
        flags=re.DOTALL,
    )
    lines = source.splitlines()
    fence = None
    for index, line in enumerate(lines):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})", line)
        if fence is None and marker:
            fence = (marker[1][0], len(marker[1]))
            lines[index] = ""
        elif fence is not None:
            if (
                marker
                and marker[1][0] == fence[0]
                and len(marker[1]) >= fence[1]
                and not line[marker.end() :].strip()
            ):
                fence = None
            lines[index] = ""
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


def disposition_errors(
    root: Path, registered: dict[str, set[str]], core_conditions: set[str]
) -> list[str]:
    """Reconcile the exact non-core vocabulary with reviewed static dispositions."""

    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate disposition key {key}")
            result[key] = value
        return result

    try:
        record = json.loads(
            (root / DISPOSITIONS).read_text(encoding="utf-8"),
            object_pairs_hook=unique_object,
        )
    except (OSError, UnicodeError, ValueError) as error:
        return [f"{DISPOSITIONS}: unavailable vocabulary dispositions: {error}"]
    if (
        not isinstance(record, dict)
        or set(record) != {"version", "scope", "conditions"}
        or type(record["version"]) is not int
        or record["version"] != 1
        or not isinstance(record["scope"], str)
        or not record["scope"].strip()
        or not isinstance(record["conditions"], dict)
    ):
        return [f"{DISPOSITIONS}: invalid vocabulary dispositions"]
    entries = record["conditions"]
    errors = []
    for condition in sorted(set(registered) - core_conditions - set(entries)):
        errors.append(
            f"vocabulary:{condition}: registered condition has no core cause "
            "or explicit disposition"
        )
    for condition, entry in sorted(entries.items()):
        prefix = f"vocabulary:{condition}"
        if condition not in registered:
            errors.append(f"{prefix}: disposition names an unregistered condition")
        elif condition in core_conditions:
            errors.append(
                f"{prefix}: core-owned condition cannot retain a non-core disposition"
            )
        if not isinstance(entry, dict) or set(entry) != {
            "kind",
            "phases",
            "reason",
            "owners",
            "controls",
            "producer",
            "history",
        }:
            errors.append(f"{prefix}: invalid disposition fields")
            continue
        if not isinstance(entry["kind"], str) or entry["kind"] not in DISPOSITION_KINDS:
            errors.append(f"{prefix}: invalid disposition kind")
        phases = entry["phases"]
        if (
            not isinstance(phases, list)
            or any(not isinstance(phase, str) for phase in phases)
            or len(phases) != len(set(phases))
            or set(phases) != registered.get(condition, set())
        ):
            errors.append(f"{prefix}: disposition phases differ from vocabulary")
        for field in ("reason", "history"):
            if not isinstance(entry[field], str) or not entry[field].strip():
                errors.append(f"{prefix}: missing {field}")
        safe_owners = set()
        for field in ("owners", "controls"):
            paths = entry[field]
            if (
                not isinstance(paths, list)
                or not paths
                or any(not isinstance(path, str) for path in paths)
            ):
                errors.append(f"{prefix}: missing {field}")
                continue
            for relative in paths:
                path = Path(relative)
                if (
                    path.is_absolute()
                    or ".." in path.parts
                    or "\\" in relative
                    or not relative.startswith(
                        ("rules/", "rust/", ".github/", "R/", "python/")
                    )
                    or path.as_posix() != relative
                    or not (root / path).is_file()
                ):
                    errors.append(f"{prefix}: unavailable {field} path {relative}")
                elif field == "owners":
                    safe_owners.add(relative)
        producer = entry["producer"]
        if entry["kind"] == "validator":
            if (
                not isinstance(producer, dict)
                or set(producer) != {"path", "symbol"}
                or not all(
                    isinstance(value, str) and value.strip()
                    for value in producer.values()
                )
            ):
                errors.append(
                    f"{prefix}: validator disposition requires its actual producing owner"
                )
            else:
                path = Path(producer["path"])
                try:
                    source = (
                        (root / path).read_text(encoding="utf-8")
                        if producer["path"] in safe_owners
                        else ""
                    )
                except (OSError, UnicodeError):
                    source = ""
                try:
                    nodes = ast.walk(ast.parse(source))
                    functions = [
                        node
                        for node in nodes
                        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
                        and node.name == producer["symbol"]
                    ]
                except SyntaxError:
                    functions = []
                identifier = re.compile(rf"(?<!\w){re.escape(condition)}(?!\w)")

                def owns_refusal(function, identifier=identifier):
                    pending = list(function.body)
                    while pending:
                        node = pending.pop()
                        if isinstance(
                            node,
                            (
                                ast.FunctionDef,
                                ast.AsyncFunctionDef,
                                ast.ClassDef,
                                ast.Lambda,
                            ),
                        ):
                            continue
                        if (
                            isinstance(node, ast.Raise)
                            and node.exc is not None
                            and any(
                                isinstance(value, ast.Constant)
                                and isinstance(value.value, str)
                                and identifier.search(value.value)
                                for value in ast.walk(node.exc)
                            )
                        ):
                            return True
                        pending.extend(ast.iter_child_nodes(node))
                    return False

                if not any(owns_refusal(function) for function in functions):
                    errors.append(f"{prefix}: unavailable actual producing owner")
        elif producer is not None:
            errors.append(
                f"{prefix}: only validator dispositions may supply an explicit producer"
            )
        if entry["kind"] == "capability" and not any(
            condition in (root / path).read_text(encoding="utf-8")
            for path in safe_owners
        ):
            errors.append(
                f"{prefix}: capability disposition lacks its named normative owner"
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
    source: str, registered: dict[str, set[str]]
) -> dict[int, list[str]]:
    """Keep exact Rust strings or JSON values with diagnostic spellings."""
    found = {}
    for line, literal in rust_literals(source):
        candidates = (
            [literal]
            if REQUIREMENT.fullmatch(literal) or literal in registered
            else embedded_json_strings(literal)
        )
        selected = [
            value
            for value in candidates
            if REQUIREMENT.fullmatch(value) or value in registered
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


def baseline_errors(
    root: Path, reference: str, seed_path: Path = SEED, allowlist_path: Path = ALLOWLIST
) -> list[str]:
    """Compare records directly with the prior revision, including retired sites."""
    command = subprocess.run(
        [
            "git",
            "ls-tree",
            "--name-only",
            reference,
            "--",
            seed_path.as_posix(),
            allowlist_path.as_posix(),
            CHECKER.as_posix(),
        ],
        cwd=root,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if command.returncode:
        return [f"diagnostic literal baseline {reference}: unavailable revision"]
    present = set(command.stdout.splitlines())

    def previous(relative: Path):
        result = subprocess.run(
            ["git", "show", f"{reference}:{relative.as_posix()}"],
            cwd=root,
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        if result.returncode:
            raise ValueError(f"unavailable {relative}")
        return result.stdout

    try:
        if (
            seed_path.as_posix() not in present
            and allowlist_path.as_posix() not in present
        ):
            # The first guard PR deliberately introduces both files. Once the
            # old checker refers to them, absence is corruption, not bootstrap.
            old_checker = previous(CHECKER) if CHECKER.as_posix() in present else ""
            if seed_path.name in old_checker or allowlist_path.name in old_checker:
                return [
                    f"diagnostic literal baseline {reference}: existing guard is missing its seed and allowlist"
                ]
            return []
        if (
            seed_path.as_posix() not in present
            or allowlist_path.as_posix() not in present
        ):
            return [
                f"diagnostic literal baseline {reference}: incomplete seed and allowlist"
            ]
        frozen = json.loads(previous(seed_path))
        prior = json.loads(previous(allowlist_path))
        current_seed = json.loads((root / seed_path).read_text(encoding="utf-8"))
        current = json.loads((root / allowlist_path).read_text(encoding="utf-8"))
        if not isinstance(prior, dict) or not isinstance(current, dict):
            raise TypeError("expected allowlist objects")
    except (OSError, UnicodeError, ValueError, TypeError) as error:
        return [f"diagnostic literal baseline {reference}: invalid records: {error}"]
    errors = []
    if frozen != current_seed:
        errors.append(f"{seed_path}: frozen literal seed changed from {reference}")
    for identifier, count in current.items():
        previous_count = prior.get(identifier, 0)
        if not (
            type(count) is int
            and type(previous_count) is int
            and count <= previous_count
        ):
            errors.append(
                f"{allowlist_path}:{identifier}: literal allowlist grew from {reference}"
            )
    return errors


def load_sites(
    root: Path,
    seed_path: Path = SEED,
    allowlist_path: Path = ALLOWLIST,
    crates: set[str] = HOST_CRATES,
) -> tuple[dict, dict, list[str]]:
    """Admit only active counts bounded by the frozen, location-aware seed."""
    errors = []
    documents = []
    for relative in (seed_path, allowlist_path):
        try:
            document = json.loads((root / relative).read_text(encoding="utf-8"))
            if not isinstance(document, dict):
                raise TypeError("expected an object")
        except (OSError, UnicodeError, ValueError, TypeError) as error:
            errors.append(f"{relative}: invalid diagnostic literal sites: {error}")
            document = {}
        documents.append(document)
    seed, active = documents
    sites = {}
    for identifier, site in seed.items():
        if not (
            isinstance(site, dict)
            and set(site) == {"path", "source", "occurrences"}
            and isinstance(site["path"], str)
            and isinstance(site["source"], str)
            and site["source"] == site["source"].strip()
            and type(site["occurrences"]) is int
            and site["occurrences"] > 0
            and re.fullmatch(
                r"rust/crates/(?:"
                + "|".join(re.escape(crate) for crate in sorted(crates))
                + r")/src/.+\.rs",
                site["path"],
            )
            and identifier.startswith(site["path"] + ":")
            and identifier.removeprefix(site["path"] + ":").isdigit()
        ):
            errors.append(f"{seed_path}:{identifier}: invalid frozen literal site")
            continue
        key = (site["path"], site["source"])
        if key in sites:
            errors.append(f"{seed_path}:{identifier}: duplicate frozen literal site")
        sites[key] = identifier
    admitted = {}
    for identifier, count in active.items():
        site = seed.get(identifier)
        if not (
            isinstance(site, dict)
            and type(count) is int
            and count > 0
            and type(site.get("occurrences")) is int
            and count <= site["occurrences"]
        ):
            errors.append(
                f"{allowlist_path}:{identifier}: literal allowlist may only shrink"
            )
        else:
            admitted[identifier] = count
    return sites, admitted, errors


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
    root: Path, registered: dict[str, set[str]], nullable: set[tuple[str, str]]
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
    core_conditions = set()
    for line in block[1].splitlines():
        if not line.strip() or line.lstrip().startswith("//"):
            continue
        cause = CAUSE.fullmatch(line)
        if cause is None:
            errors.append(
                f"core diagnostic registry: unrecognized cause: {line.strip()}"
            )
            continue
        count += 1
        name, phase, condition, requirement = cause.groups()
        core_conditions.add(condition)
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
    errors.extend(disposition_errors(root, registered, core_conditions))
    return errors


def violations(root: Path, baseline_ref: str | None = None) -> list[str]:
    """Check every mapping and exact existing literals outside the canonical registry."""
    defined = {
        identifier
        for path in (root / "rules").rglob("*.md")
        for identifier in DEFINITION.findall(path.read_text(encoding="utf-8"))
    }
    if not defined:
        return ["no normative requirement definitions found"]
    sources = sorted((root / "rust/crates").glob("*/src/**/*.rs"))
    if not sources:
        return ["no Rust crate sources found"]
    registered, errors = vocabulary(root)
    nullable, null_errors = null_conditions(root, registered)
    errors.extend(null_errors)
    errors.extend(registry_errors(root, registered, nullable))
    errors.extend(rule_vocabulary_errors(root, registered))
    namespaces = {}
    for seed_path, allowlist_path, crates in (
        (SEED, ALLOWLIST, HOST_CRATES),
        (CORE_SEED, CORE_ALLOWLIST, CORE_CRATES),
    ):
        sites, active, site_errors = load_sites(root, seed_path, allowlist_path, crates)
        errors.extend(site_errors)
        if baseline_ref is not None:
            errors.extend(
                baseline_errors(root, baseline_ref, seed_path, allowlist_path)
            )
        namespaces[seed_path] = (sites, active, Counter(), allowlist_path)
    for path in sources:
        relative = path.relative_to(root).as_posix()
        crate = path.relative_to(root / "rust/crates").parts[0]
        namespace = CORE_SEED if crate in CORE_CRATES else SEED
        sites, active, used, _ = namespaces[namespace]
        source = path.read_text(encoding="utf-8")
        lines = source.splitlines()
        test_only = private_unit_test_source(root, Path(relative))
        for number, literals in diagnostic_literals(source, registered).items():
            line = lines[number - 1]
            for identifier in literals:
                if REQUIREMENT.fullmatch(identifier) and identifier not in defined:
                    errors.append(
                        f"{relative}:{number}: undefined requirement {identifier}"
                    )
            owned = literals
            if Path(relative) in (REGISTRY, SHIPPED_SCHEMA) or test_only or not owned:
                continue
            seed_identifier = sites.get((relative, line.strip()))
            used[seed_identifier] += 1
            if seed_identifier is None or used[seed_identifier] > active.get(
                seed_identifier, 0
            ):
                errors.append(
                    f"{relative}:{number}: new diagnostic literal site "
                    + (
                        "outside canonical registry: "
                        if namespace == CORE_SEED
                        else "outside core: "
                    )
                    + ", ".join(owned)
                )
    for _, active, used, allowlist_path in namespaces.values():
        for identifier, count in active.items():
            if type(count) is int and count > used[identifier]:
                errors.append(
                    f"{allowlist_path}:{identifier}: stale literal site; shrink the allowlist"
                )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parents[2]
    )
    parser.add_argument(
        "--baseline-ref",
        help="prior Git revision; require its frozen seed unchanged and active allowlist to only shrink",
    )
    arguments = parser.parse_args()
    errors = violations(arguments.root, arguments.baseline_ref)
    for error in errors:
        print(error)
    if errors:
        return 1
    print(
        "PASS: diagnostic mappings are registered; host, core and engine literal sites only shrink."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
