"""Editorial validators shared by the validity gate and the docs gate.

Extracted from validate_repository.py (issue #184) without behavior
changes: the six prose-oriented validators plus their leaf helpers and
constants. This module imports only the standard library and PyYAML and
must never import validate_repository (cycle risk). validate_repository.py
re-exports every public name defined here for backward compatibility.
"""

import copy
import csv
import re
from collections.abc import Hashable
from pathlib import Path

from yaml.composer import ComposerError
from yaml.constructor import ConstructorError
from yaml.events import AliasEvent

import yaml


class UniqueKeyLoader(yaml.SafeLoader):
    yaml_implicit_resolvers = copy.deepcopy(yaml.SafeLoader.yaml_implicit_resolvers)

    def compose_node(self, parent, index):
        event = self.peek_event()
        if isinstance(event, AliasEvent):
            raise ComposerError(
                None,
                None,
                "YAML aliases are not allowed",
                event.start_mark,
            )
        if getattr(event, "anchor", None) is not None:
            raise ComposerError(
                None,
                None,
                "YAML anchors are not allowed",
                event.start_mark,
            )
        if getattr(event, "tag", None) is not None:
            raise ComposerError(
                None,
                None,
                "explicit YAML tags are not allowed",
                event.start_mark,
            )
        return super().compose_node(parent, index)

    def flatten_mapping(self, node):
        for key_node, _ in node.value:
            if key_node.tag == "tag:yaml.org,2002:merge":
                raise ConstructorError(
                    None,
                    None,
                    "YAML merge keys are not allowed",
                    key_node.start_mark,
                )
        return super().flatten_mapping(node)

    def construct_mapping(self, node, deep=False):
        # SafeConstructor.construct_mapping starts with this call; without it
        # the merge-key guard in flatten_mapping above never runs and a merge
        # key dies later with "could not determine a constructor" instead of
        # the intended "YAML merge keys are not allowed".
        self.flatten_mapping(node)
        mapping = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if not isinstance(key, Hashable):
                # SafeConstructor raises this for a sequence or mapping used
                # as a key; without the guard the membership test below dies
                # with a raw TypeError that escapes the callers catching
                # yaml.YAMLError and crashes the whole validation run.
                raise ConstructorError(
                    None,
                    None,
                    "found unhashable key",
                    key_node.start_mark,
                )
            if key in mapping:
                raise ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key '{key}'",
                    key_node.start_mark,
                )
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


SPEC_FILE_PATTERN = re.compile(r"^spec(?:_[a-z][a-z0-9_]*)?\.yaml$")


def example_spec_paths(example_dir: Path):
    return sorted(
        path
        for path in example_dir.iterdir()
        if path.is_file() and SPEC_FILE_PATTERN.fullmatch(path.name)
    )


README_FORBIDDEN_PATTERN = re.compile(
    r"\b(?:derivation|schema|handler|verification)s?\b"
    r"|R0[0-9][0-9]|REQ-[0-9]+|output\.columns",
    re.IGNORECASE,
)
README_KEY_COLUMNS = {
    "STUDYID",
    "USUBJID",
    "DOMAIN",
    "SUBJID",
    "AESEQ",
    "VSSEQ",
    "LBSEQ",
    "RSSEQ",
    "ASEQ",
    "PARAMCD",
    "PARAM",
    "AVISIT",
    "VISIT",
    "RDOMAIN",
    "IDVAR",
    "QNAM",
}


def validate_example_readmes(root: Path):
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue
        readme_path = ex_dir / "README.md"
        if not readme_path.exists():
            continue
        label = readme_path.relative_to(root)
        text = readme_path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), 1):
            if len(line) > 79:
                errors.append(
                    f"ERROR: {label}:{line_number}: line has {len(line)} "
                    "characters; maximum is 79"
                )

        marker = "\n## How to fix\n"
        contract = text.split(marker, 1)[0]
        for line_number, line in enumerate(contract.splitlines(), 1):
            if README_FORBIDDEN_PATTERN.search(line):
                errors.append(
                    f"ERROR: {label}:{line_number}: schema vocabulary is "
                    "not allowed in the data contract"
                )

        headings = [line for line in text.splitlines() if line.startswith("## ")]
        if ex_dir.name.startswith("negative-"):
            if headings != ["## How to fix"]:
                errors.append(
                    f"ERROR: {label}: negative README must contain exactly "
                    "one '## How to fix' section and no other level-two section"
                )
        else:
            invalid = [
                heading
                for heading in headings
                if heading != "## Specification variants"
            ]
            if invalid:
                errors.append(
                    f"ERROR: {label}: unsupported level-two section(s): "
                    + ", ".join(invalid)
                )
            if (
                "## Specification variants" in headings
                and len(example_spec_paths(ex_dir)) < 2
            ):
                errors.append(
                    f"ERROR: {label}: Specification variants requires at "
                    "least two variant specs"
                )

        for csv_path in sorted((ex_dir / "expected").glob("*.csv")):
            try:
                with open(csv_path, "r", encoding="utf-8", newline="") as f:
                    header = next(csv.reader(f, strict=True), [])
            except (OSError, UnicodeError, csv.Error):
                continue
            missing = [
                name
                for name in header
                if name not in README_KEY_COLUMNS and name not in text
            ]
            if missing:
                errors.append(
                    f"ERROR: {label}: expected columns not described: "
                    + ", ".join(missing)
                )
    return errors


def rule_identity_errors(meta: dict, stem: str, label: str) -> list[str]:
    errors = []
    if meta.get("id") != stem:
        errors.append(f"ERROR: {label}: id must be {stem!r}, got {meta.get('id')!r}")
    if meta.get("status") != "normative":
        errors.append(f"ERROR: {label}: maintained rule status must be 'normative'")
    return errors


def validate_rule_metadata(root: Path):
    if any((root / "rules").glob("*/*.md")):
        from tools.validation.check_rule_rewrite import check

        return check(root)[0]
    errors = []
    rules_dir = root / "rules"
    index_path = rules_dir / "README.md"
    if not rules_dir.is_dir() or not index_path.is_file():
        return errors
    index = index_path.read_text(encoding="utf-8")

    for rule_path in sorted(rules_dir.glob("R[0-9][0-9][0-9]-*.md")):
        label = rule_path.relative_to(root)
        text = rule_path.read_text(encoding="utf-8")
        match = re.match(r"\A---\n(.*?)\n---\n", text, re.DOTALL)
        if match is None:
            errors.append(f"ERROR: {label}: missing YAML front matter")
            continue
        try:
            metadata = yaml.load(match.group(1), Loader=UniqueKeyLoader)
        except yaml.YAMLError as exc:
            errors.append(f"ERROR: {label}: invalid front matter: {exc}")
            continue
        if not isinstance(metadata, dict):
            errors.append(f"ERROR: {label}: front matter must be a mapping")
            continue

        expected_id = rule_path.name[:4]
        errors.extend(rule_identity_errors(metadata, expected_id, label))

        index_row = re.search(
            rf"^\| {re.escape(expected_id)} \|.*?\| ([^|]+) \|",
            index,
            re.MULTILINE,
        )
        if index_row is None:
            errors.append(f"ERROR: {label}: rule is absent from rules/README.md")
        elif index_row.group(1).strip() != "normative":
            errors.append(
                f"ERROR: rules/README.md: {expected_id} status must be 'normative'"
            )

    return errors


ASCII_SOURCE_SUFFIXES = {
    ".csv",
    ".json",
    ".md",
    ".py",
    ".r",
    ".rd",
    ".sh",
    ".toml",
    ".txt",
    ".yaml",
    ".yml",
}
ASCII_SOURCE_NAMES = {"DESCRIPTION", "NAMESPACE"}
# Preserve authoritative third-party data/notices byte for byte, including their
# Unicode copyright text. This is a closed list, not a general vendor exemption.
UNICODE_DATA_AND_NOTICES = {
    "rust/crates/yamaa-core/unicode/18.0.0/DerivedCoreProperties.txt",
    "rust/crates/yamaa-core/unicode/LICENSE.txt",
    "rust/crates/yamaa-python/LICENSE-UNICODE.txt",
    "R/yamaa/inst/LICENSE-UNICODE.txt",
    "python/LICENSE-UNICODE.txt",
    # Generated from the crates' own license, copyright and NOTICE files.
    "R/yamaa/inst/LICENSE-THIRD-PARTY.txt",
    "python/LICENSE-THIRD-PARTY.txt",
}
ASCII_SOURCE_IGNORED_PARTS = {
    ".git",
    ".pytest_cache",
    ".venv",
    ".venv-docs",
    "__pycache__",
    "venv",
}


def is_unicode_fixture_data(relative: Path):
    parts = relative.parts
    benchmark_csv = (
        relative.suffix.lower() == ".csv"
        and len(parts) >= 4
        and parts[0] == "benchmarks"
        and parts[2] in {"input", "expected"}
    )
    rust_contract_json = (
        relative.suffix.lower() == ".json"
        and len(parts) >= 6
        and parts[:2] == ("rust", "crates")
        and parts[3:5] == ("tests", "fixtures")
    )
    return benchmark_csv or rust_contract_json


def validate_ascii_sources(root: Path):
    """Check project source bytes while preserving named Unicode data and notices."""
    errors = []
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(root)
        if any(part in ASCII_SOURCE_IGNORED_PARTS for part in relative.parts):
            continue
        if is_unicode_fixture_data(relative):
            continue
        if relative.as_posix() in UNICODE_DATA_AND_NOTICES:
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


def validate_examples_index(root: Path):
    errors = []
    examples_dir = root / "benchmarks"
    index_file = examples_dir / "README.md"
    if not index_file.exists():
        return errors

    index_content = index_file.read_text(encoding="utf-8")
    if not re.search(r"^\| \[`", index_content, re.MULTILINE):
        # The committed README carries no index table: the index is derived
        # at docs build time, so validate the rendered index instead.

        try:
            from tools.benchmark_docs.build_index import render_index

            index_content = render_index(examples_dir, index_content)
        except (OSError, ValueError) as exc:
            return [f"ERROR: cannot derive benchmark index: {exc}"]
    # Find all table rows matching: | [`dir`](dir/) | desc | ... |
    # Trailing columns (lifecycle, conformance, ...) are allowed and ignored.
    # Column classes exclude newlines so a match can never span rows.
    pattern = re.compile(
        r"^\|\s*\[`([^`\n]+)`\]\(([^)\n]+)\)\s*\|\s*([^\n|]+?)\s*(?:\|[^\n|]*?)*\|$",
        re.MULTILINE,
    )

    indexed_entries = {}
    last_dir = None
    not_alphabetical = False

    for match in pattern.finditer(index_content):
        dname = match.group(1)
        link_target = match.group(2)
        desc = match.group(3).strip()

        if link_target != f"{dname}/":
            errors.append(
                f"ERROR: index entry {dname} links to {link_target}, expected {dname}/"
            )

        if dname in indexed_entries:
            errors.append(f"ERROR: duplicate index entry for {dname}")

        if last_dir and dname < last_dir and not not_alphabetical:
            errors.append(
                f"ERROR: index entries not alphabetical ({last_dir} before {dname})"
            )
            not_alphabetical = True
        last_dir = dname

        indexed_entries[dname] = desc

        ex_dir = examples_dir / dname
        if not ex_dir.is_dir():
            errors.append(
                f"ERROR: index entry {dname} is stale (directory does not exist)"
            )

    # Check for missing entries and title contract
    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue

        dname = ex_dir.name
        if dname not in indexed_entries:
            errors.append(f"ERROR: example {dname} not in index")
            continue

        readme = ex_dir / "README.md"
        if readme.exists():
            content = readme.read_text(encoding="utf-8")
            first_line = content.split("\n", 1)[0] if content else ""
            if ":" in first_line:
                title_desc = first_line.split(":", 1)[1].strip()
                # The derived index (build_index) carries the full title,
                # while a committed table may carry only the part after the
                # colon; either form satisfies the title contract.
                full_title = first_line.removeprefix("#").strip()
                if indexed_entries[dname] not in (title_desc, full_title):
                    errors.append(
                        f"ERROR: {dname} title description '{title_desc}' does not match index '{indexed_entries[dname]}'"
                    )

    return errors


def validate_examples_readme_presence(root: Path):
    """Require each example's README and each negative README's fix section.

    Documentation lint: lives behind check_documentation.py, not the
    specification-validity gate, so a prose gap cannot mask a spec verdict.
    """
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue

        rel = ex_dir.relative_to(root)
        readme = ex_dir / "README.md"
        if not readme.exists():
            errors.append(f"ERROR: {rel} missing README.md")
        elif ex_dir.name.startswith("negative-"):
            content = readme.read_text(encoding="utf-8")
            if "## How to fix" not in content:
                errors.append(f"ERROR: {rel}/README.md missing '## How to fix' section")

    return errors


def validate_column_labels(spec, spec_label):
    """Require every surviving declared column to carry a usable label."""
    errors = []
    columns = spec.get("columns")
    if not isinstance(columns, list):
        return errors
    for index, column in enumerate(columns):
        if not isinstance(column, dict):
            continue
        label = column.get("label")
        if isinstance(label, str) and label.strip():
            continue
        errors.append(
            f"ERROR: {spec_label}.columns[{index}].label: declared column "
            "requires a non-empty label"
        )
    return errors


def validate_producer_output_contract(producer, path):
    """Require each stored producer column to carry a usable label."""
    errors = []
    output = producer.get("output")
    output_columns = output.get("columns") if isinstance(output, dict) else None
    selected = (
        {name for name in output_columns if isinstance(name, str)}
        if isinstance(output_columns, list)
        else set()
    )
    columns = producer.get("columns")
    if not isinstance(columns, list):
        return errors
    for index, column in enumerate(columns):
        if not isinstance(column, dict) or column.get("name") not in selected:
            continue
        label = column.get("label")
        if not isinstance(label, str) or not label.strip():
            errors.append(
                f"ERROR: {path}.columns[{index}].label: stored producer "
                "column requires a non-empty label"
            )
    return errors
