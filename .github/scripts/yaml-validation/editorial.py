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
        mapping = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
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
    r'\b(?:derivation|schema|handler|verification)s?\b'
    r'|R0[0-9][0-9]|REQ-[0-9]+|output\.columns',
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
README_FOOTER_PATTERN = re.compile(
    r"\[!\[Dashboard\]\(https://img\.shields\.io/badge/Dashboard-view-1f3a5c\)\]"
    r"\(https://elong0527\.github\.io/yamaa/benchmark/"
    r"[a-z0-9]+(?:-[a-z0-9]+)*\.html\)",
)
LIFECYCLE_BADGE_PATTERN = re.compile(
    r"\[!\[Lifecycle: (draft|reviewed|finalized)\]"
    r"\(https://img\.shields\.io/badge/Lifecycle-[^)]+\)\]"
    r"\([^)]*\)"
)


def is_readme_badge_line(text: str) -> bool:
    """Accept a Dashboard badge alone, a lifecycle badge alone,
    or the Dashboard badge followed by one lifecycle badge."""
    stripped = text.strip()
    if README_FOOTER_PATTERN.fullmatch(stripped):
        return True
    if LIFECYCLE_BADGE_PATTERN.fullmatch(stripped):
        return True
    match = README_FOOTER_PATTERN.match(stripped)
    if not match:
        return False
    rest = stripped[match.end() :]
    if not rest.startswith(" "):
        return False
    return bool(LIFECYCLE_BADGE_PATTERN.fullmatch(rest[1:]))


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
            if len(line) > 79 and not is_readme_badge_line(line):
                errors.append(
                    f"ERROR: {label}:{line_number}: line has {len(line)} "
                    "characters; maximum is 79"
                )

        marker = "\n## How to fix\n"
        contract = text.split(marker, 1)[0]
        for line_number, line in enumerate(contract.splitlines(), 1):
            if is_readme_badge_line(line):
                continue
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
    if (root / 'rules/migration.yaml').is_file():
        from check_rule_rewrite import check
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
ASCII_SOURCE_IGNORED_PARTS = {
    ".git",
    ".pytest_cache",
    ".venv",
    ".venv-docs",
    "__pycache__",
    "venv",
}


def is_unicode_fixture_csv(relative: Path):
    parts = relative.parts
    return (
        relative.suffix.lower() == ".csv"
        and len(parts) >= 4
        and parts[0] == "benchmarks"
        and parts[2] in {"input", "expected"}
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


def validate_examples_index(root: Path):
    errors = []
    examples_dir = root / "benchmarks"
    index_file = examples_dir / "README.md"
    if not index_file.exists():
        return errors

    index_content = index_file.read_text(encoding="utf-8")
    # Find all table rows matching: | [`dir`](dir/) | desc | (Lifecycle) |
    pattern = re.compile(
        r"^\|\s*\[`([^`]+)`\]\(([^)]+)\)\s*\|\s*([^|]+?)\s*(?:\|\s*([^|]*?)\s*)?\|$",
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
                if title_desc != indexed_entries[dname]:
                    errors.append(
                        f"ERROR: {dname} title description '{title_desc}' does not match index '{indexed_entries[dname]}'"
                    )

    return errors


def validate_examples_badges(root: Path):
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue

        rel = ex_dir.relative_to(root)
        readme = ex_dir / "README.md"
        if readme.exists():
            lines = readme.read_text(encoding="utf-8").splitlines()
            expected_badge = (
                "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)]"
                f"(https://elong0527.github.io/yamaa/benchmark/{ex_dir.name}.html)"
            )
            badge_ok = False
            if len(lines) >= 3:
                stripped = lines[2].strip()
                if stripped.startswith(expected_badge):
                    # Same line: Dashboard badge + lifecycle badge,
                    # or one badge per line: lifecycle badge on the next line.
                    rest = stripped[len(expected_badge) :]
                    if rest.startswith(" "):
                        badge_ok = bool(LIFECYCLE_BADGE_PATTERN.fullmatch(rest[1:]))
                    elif not rest and len(lines) >= 4:
                        badge_ok = bool(
                            LIFECYCLE_BADGE_PATTERN.fullmatch(lines[3].strip())
                        )
            if not badge_ok:
                errors.append(
                    f"ERROR: {rel}/README.md must place '{expected_badge}' "
                    "followed by a lifecycle badge (draft, reviewed, or "
                    "finalized) right after the title"
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
