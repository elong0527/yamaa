"""Check the rule files, every rule citation, markdown links and schema navigation.

One run reads the shared rules model (:mod:`tools.rules.model`) and checks:

- the index: its Rule files table lists every rule file once under its title,
  with a unique prefix (``REQ`` for a contract not yet converted), and each
  principle cites at least one rule;
- cards: anchor, prefix, unique ID, title, statement length, and the
  ``Fails``, ``Evidence`` and ``Replaces`` lines;
- legacy requirements: one definition each, anchors, contract front matter
  and sections, and the verification checks the verification contract defines;
- every rule ID cited anywhere in the repository, and the requirement each
  negative benchmark expects, resolves to a legacy requirement, a card or a
  card's ``Replaces``;
- relative markdown links name an existing file and anchor, and a link whose
  text is a rule ID targets that rule;
- schema descriptions and comments cite rules that resolve;
- rules prose states behavior without rationale phrases.

It also prints, without failing, the registered conditions that no card's
``Fails`` line or legacy requirement names.
"""

import argparse
import posixpath
import re
import textwrap
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

import yaml
from tools.docs.pages import CANONICAL, GENERATED, GENERATED_DIRECTORIES
from tools.paths import ROOT
from tools.rules.migrate import SENTINELS
from tools.rules.model import (
    GENERATED_PAGES,
    LEGACY_PREFIX,
    METADATA_LABELS,
    anchors,
    authored_files,
    load,
    markdown_links,
    prose_lines,
    words,
)
from tools.validation.editorial import UniqueKeyLoader
from tools.validation.repository_schema import (
    POINTER,
    SchemaSourceError,
    description_rules,
    load_schema,
    schema_descriptions,
)

INDEX = "rules/README.md"
STATEMENT_WORDS = 90
CARD_NUMBER = re.compile(r"[1-9]\d*")
CONDITION = re.compile(r"`(?P<name>[a-z][a-z0-9_]*)`")
EVIDENCE = re.compile(r"`(?P<item>[^`\s]+)`")
REPLACED = re.compile(r"REQ-\d{4}")
RETIRED = re.compile(r"(?:this form is )?retired\b", re.IGNORECASE)
CONTRACT_SECTIONS = (["Requirements"], ["Requirements", "Error conditions"])
SCHEMA_COMMENT = re.compile(r"(?:^\s*#|\s+#)\s*([^\n]*)", re.MULTILINE)

VERIFICATION_CONTRACT = "verification.md"
VERIFICATION_REGISTRIES = (
    "column_verifications",
    "dataset_verifications",
    "intermediate_verifications",
)
# A rule defines a verification when it opens with the check's name, as in
# "`unique` requires ...".
DEFINING_NAME = re.compile(r"(?:An?\s+|The\s+)?`([a-z][a-z0-9_]*)`")

# Rules state what is expected; these phrases signal rationale prose.
RATIONALE_PHRASES = (
    "because",
    "in order to",
    "the reason is",
    "this ensures",
    "so that",
    "to ensure",
)
RATIONALE = re.compile(rf"\b(?:{'|'.join(RATIONALE_PHRASES)})\b", re.IGNORECASE)
# rules-relative path -> [(line pattern, why it cannot be reworded)].
# Prefer rewording over allowlisting.
RATIONALE_ALLOWLIST = {}

CITATION_SUFFIXES = frozenset(
    {
        ".csv",
        ".html",
        ".js",
        ".json",
        ".md",
        ".py",
        ".r",
        ".rd",
        ".rs",
        ".sas",
        ".toml",
        ".tsv",
        ".txt",
        ".xml",
        ".yaml",
        ".yml",
    }
)
CITATION_NAMES = ("DESCRIPTION", "Makefile", "NAMESPACE")
# Files that build invalid rule IDs on purpose, or hold data that reads like
# a rule ID. None exempts the whole file.
CITATION_EXEMPT = {
    # Benchmark subject identifiers that read like TEXT card IDs.
    "benchmarks/schema-text-functions/expected/adsl.csv": None,
    "benchmarks/schema-text-functions/input/dm.csv": None,
    # SAS arithmetic on a variable named VALUE.
    "evaluations/harbor/solutions/sdtm-vs-units/result.sas": frozenset({"VALUE-32"}),
    # A synthetic diagnostic record that names no rule, and this table.
    "python/tests/test_native_results.py": frozenset({"REQ-0000"}),
    "tools/rules/check.py": frozenset({"REQ-0000", "VALUE-32"}),
    # The sentinel IDs the citations command leaves as written.
    "tools/rules/migrate.py": SENTINELS,
    "tools/tests/test_diagnostics.py": None,
    "tools/tests/test_rule_reference.py": None,
    "tools/tests/test_rules_check.py": None,
    "tools/tests/test_rules_migrate.py": None,
    "tools/tests/test_rules_model.py": None,
}
# A template the benchmark dashboard generator fills; not a page itself.
LINK_EXEMPT = frozenset({"docs/articles/benchmark.md"})


@dataclass
class Report:
    rules: object
    errors: list
    unnamed_conditions: list


def registered_conditions(root):
    """The condition names ``yaml/conditions.yaml`` registers, or None."""
    try:
        document = yaml.safe_load((root / "yaml/conditions.yaml").read_text("utf-8"))
    except (OSError, UnicodeError, yaml.YAMLError):
        return None
    conditions = document.get("conditions") if isinstance(document, dict) else None
    return set(conditions) if isinstance(conditions, dict) else None


# -- the index ----------------------------------------------------------------


def index_errors(rules):
    """The Rule files table lists every rule file once with a valid prefix."""
    if rules.readme is None:
        return [f"{INDEX}: missing rule index"]
    if rules.listing is None:
        return [f"{INDEX}: no Rule files table (| File | Prefix | Covers |)"]
    errors = [f"{INDEX}:{line}: {problem}" for line, problem in rules.listing_problems]
    listed = {}
    prefixes = {}
    for row in rules.listing:
        where = f"{INDEX}:{row.line}"
        if row.path in listed:
            errors.append(f"{where}: {row.path} is listed twice")
            continue
        listed[row.path] = row
        rule_file = rules.files.get(row.path)
        if rule_file is None:
            errors.append(f"{where}: {row.path} is not a rule file")
            continue
        if not re.fullmatch(r"[A-Z]+", row.prefix):
            errors.append(f"{where}: prefix {row.prefix!r} must be capital letters")
        elif row.prefix != LEGACY_PREFIX:
            if row.prefix in prefixes:
                errors.append(
                    f"{where}: prefix {row.prefix} is already used by "
                    f"{prefixes[row.prefix]}"
                )
            prefixes.setdefault(row.prefix, row.path)
        if rule_file.title is not None and row.title != rule_file.title:
            errors.append(
                f"{where}: {row.path} is listed as {row.title!r} "
                f"but titled {rule_file.title!r}"
            )
        if not row.covers:
            errors.append(f"{where}: {row.path} needs a Covers description")
        if rule_file.kind == "legacy" and row.prefix != LEGACY_PREFIX:
            errors.append(
                f"{where}: {row.path} holds legacy requirements and is listed "
                f"with prefix {LEGACY_PREFIX} until it is converted"
            )
        if rule_file.kind == "cards" and row.prefix == LEGACY_PREFIX:
            errors.append(
                f"{where}: {row.path} holds cards and needs its own prefix, "
                f"not {LEGACY_PREFIX}"
            )
    for path in sorted(set(rules.files) - set(listed)):
        errors.append(f"rules/{path}: absent from the Rule files table in {INDEX}")
    return errors


def principle_errors(rules):
    """Principles are numbered in order and each cites at least one rule.

    Whether the cited IDs resolve is checked with every other citation.
    """
    if rules.readme is None:
        return []
    if not rules.principles:
        return [
            f"{INDEX}: no principles (**P1. Title.** paragraphs under ## Principles)"
        ]
    errors = []
    for expected, principle in enumerate(rules.principles, 1):
        where = f"{INDEX}:{principle.line}"
        if principle.number != expected:
            errors.append(f"{where}: P{principle.number} should be P{expected}")
        if not any(rules.citations(principle.text)):
            errors.append(f"{where}: P{principle.number} cites no rule")
    return errors


# -- rule files ---------------------------------------------------------------


def file_errors(rules):
    """Each rule file holds cards or legacy requirements, never both."""
    errors = []
    for rule_file in rules.files.values():
        label = f"rules/{rule_file.path}"
        if rule_file.title is None:
            errors.append(f"{label}: no '# Title' heading")
        if rule_file.kind == "mixed":
            errors.append(
                f"{label}: mixes cards and legacy requirements; convert the whole file"
            )
        elif rule_file.kind == "empty":
            errors.append(f"{label}: defines no cards or requirements")
        elif rule_file.kind == "cards":
            if rule_file.front_matter is not None:
                errors.append(f"{label}: a card file must not have front matter")
            errors.extend(
                f"{label}:{heading.line}: '### {heading.text}' is not a card "
                "heading ('### PREFIX-N. Title')"
                for heading in rule_file.stray_headings
            )
        else:
            errors.extend(contract_errors(rule_file))
    return errors


def contract_errors(rule_file):
    """Front matter, sections, anchors and live blocks of a legacy contract."""
    label = f"rules/{rule_file.path}"
    errors = []
    meta = None
    if rule_file.front_matter is not None:
        try:
            meta = yaml.load(
                rule_file.front_matter.strip("-\n"), Loader=UniqueKeyLoader
            )
        except yaml.YAMLError as exc:
            errors.append(f"{label}: invalid contract front matter: {exc}")
    title = meta.get("title") if isinstance(meta, dict) else None
    expected = {
        "id": rule_file.path.removesuffix(".md"),
        "title": title,
        "status": "normative",
    }
    if meta != expected or not isinstance(title, str) or not title:
        errors.append(f"{label}: expected id, title, and normative status")
    sections = [heading.text for heading in rule_file.headings if heading.level == 2]
    if sections not in CONTRACT_SECTIONS:
        errors.append(f"{label}: incorrect contract section order")
    for requirement in rule_file.requirements:
        where = f"{label}:{requirement.line}"
        if RETIRED.match(requirement.text):
            errors.append(
                f"{where}: retired requirement must be deleted: {requirement.id}"
            )
        if requirement.anchor != requirement.id.lower():
            errors.append(
                f'{where}: {requirement.id} needs <a id="{requirement.id.lower()}"></a>'
                " directly above it"
            )
    return errors


def requirement_errors(rules):
    """Every legacy requirement is defined exactly once."""
    defined = {}
    for requirement in rules.all_requirements():
        defined.setdefault(requirement.id, []).append(requirement.file)
    return [
        f"duplicate requirement definition: {identifier} "
        f"({len(files)}x in {', '.join(sorted(set(files)))})"
        for identifier, files in sorted(defined.items())
        if len(files) > 1
    ]


def evidence_errors(root, item):
    """Why one Evidence item names nothing, or None when it exists.

    A bare name is a benchmark directory; anything with a slash or dot is a
    repository-relative path, optionally ``path::name`` with ``name`` in it.
    """
    path, separator, name = item.partition("::")
    if separator and not name:
        return f"{item} names nothing after '::'"
    if "/" not in path and "." not in path:
        if separator:
            return f"{item}: a benchmark has no '::' name"
        if not (root / "benchmarks" / path).is_dir():
            return f"{item} is not a benchmark under benchmarks/"
        return None
    relative = PurePosixPath(path)
    if relative.is_absolute() or ".." in relative.parts:
        return f"{item} must be a repository-relative path"
    target = root / relative
    if not target.exists():
        return f"{path} does not exist"
    if separator:
        if not target.is_file():
            return f"{path} is not a file that can hold {name!r}"
        if name not in target.read_text(encoding="utf-8", errors="replace"):
            return f"{name!r} does not occur in {path}"
    return None


def metadata_errors(root, card, conditions):
    errors = []
    labels = [entry.label for entry in card.metadata]
    for entry in card.metadata:
        where = f"rules/{card.file}:{entry.line}: {card.id}"
        if labels.count(entry.label) > 1:
            errors.append(f"{where}: repeats {entry.label}")
        if not entry.value.endswith("."):
            errors.append(f"{where}: the {entry.label} line must end with a period")
        if not entry.items or not all(entry.items):
            errors.append(f"{where}: {entry.label} must list comma-separated items")
            continue
        for item in entry.items:
            if entry.label == "Fails":
                condition = CONDITION.fullmatch(item)
                if condition is None:
                    errors.append(f"{where}: Fails item {item!r} is not a `condition`")
                elif conditions is not None and condition["name"] not in conditions:
                    errors.append(
                        f"{where}: {condition['name']} is not registered in "
                        "yaml/conditions.yaml"
                    )
            elif entry.label == "Evidence":
                evidence = EVIDENCE.fullmatch(item)
                problem = (
                    f"Evidence item {item!r} is not a `benchmark` or `path`"
                    if evidence is None
                    else evidence_errors(root, evidence["item"])
                )
                if problem:
                    errors.append(f"{where}: {problem}")
            elif REPLACED.fullmatch(item) is None:
                errors.append(f"{where}: Replaces item {item!r} is not a REQ-NNNN ID")
    order = [METADATA_LABELS.index(label) for label in dict.fromkeys(labels)]
    if order != sorted(order):
        errors.append(
            f"rules/{card.file}:{card.metadata[0].line}: {card.id}: metadata "
            "must be ordered Fails, Evidence, Replaces"
        )
    return errors


def card_errors(root, rules, conditions):
    """Structure of every card, and IDs and replacements across all cards."""
    errors = []
    prefixes = {
        row.path: row.prefix
        for row in rules.listing or ()
        if re.fullmatch(r"[A-Z]+", row.prefix)
    }
    if conditions is None and any(card.fails for card in rules.all_cards()):
        errors.append("yaml/conditions.yaml: cannot read the registered conditions")
    for card in rules.all_cards():
        where = f"rules/{card.file}:{card.line}: {card.id}"
        anchor = card.id.lower()
        if card.anchor is None:
            errors.append(f'{where}: needs <a id="{anchor}"></a> above its heading')
        elif card.anchor != anchor:
            errors.append(f"{where}: anchor {card.anchor!r} must be {anchor!r}")
        if CARD_NUMBER.fullmatch(card.number) is None:
            errors.append(
                f"{where}: the number must be at least 1, without leading zeros"
            )
        prefix = prefixes.get(card.file)
        if prefix not in (None, LEGACY_PREFIX) and card.prefix != prefix:
            errors.append(f"{where}: cards in {card.file} must use the prefix {prefix}")
        if not card.dotted:
            errors.append(f"{where}: the heading must read '### {card.id}. Title'")
        if not card.title:
            errors.append(f"{where}: has no title")
        elif card.title.endswith("."):
            errors.append(f"{where}: the title must not end with a period")
        if card.statement is None:
            errors.append(f"{where}: has no statement paragraph after its heading")
        elif (count := words(card.statement)) > STATEMENT_WORDS:
            errors.append(
                f"rules/{card.file}:{card.statement_line}: {card.id}: the statement "
                f"has {count} words; the limit is {STATEMENT_WORDS}"
            )
        errors.extend(
            f"rules/{card.file}:{line}: {card.id}: Fails, Evidence and Replaces must "
            "form one paragraph directly after the statement"
            for line in card.stray_metadata
        )
        errors.extend(metadata_errors(root, card, conditions))
    defined = {}
    replaced = {}
    for card in rules.all_cards():
        defined.setdefault(card.id, []).append(card)
        for identifier in card.replaces:
            replaced.setdefault(identifier, []).append(card)
    for identifier, cards in sorted(defined.items()):
        if len(cards) > 1:
            places = ", ".join(f"rules/{card.file}:{card.line}" for card in cards)
            errors.append(f"duplicate card {identifier}: {places}")
    for identifier, cards in sorted(replaced.items()):
        if len(cards) > 1:
            names = ", ".join(card.id for card in cards)
            errors.append(f"{identifier} is replaced more than once: {names}")
        requirement = rules.requirements.get(identifier)
        if requirement is not None:
            errors.append(
                f"{identifier} is replaced by {cards[0].id} but still defined at "
                f"rules/{requirement.file}:{requirement.line}"
            )
    return errors


def verification_errors(root, rules):
    """Match the checks the verification contract defines with the registries.

    A rule in the verification contract may open with a registered check or
    with a payload field one declares (`filter`, `when`); any other opening
    name defines a check the schema rejects. Every registered check must open
    at least one rule, so none is registered without its contract.
    """
    schema_path = root / "yaml/schema_verification.yaml"
    contract = rules.files.get(VERIFICATION_CONTRACT)
    if not schema_path.is_file() or contract is None:
        return []  # A partial root carries no verification vocabulary to match.
    try:
        schema = load_schema(schema_path)
    except SchemaSourceError as exc:
        return [str(exc)]
    classes = {name: value for name, value in schema.items() if isinstance(value, list)}

    def fields(definition):
        """Yield the payload fields one check declares, through named classes."""
        if isinstance(definition, list):
            for item in definition:
                if isinstance(item, dict):
                    yield from (key for key in item if key != "fields_from")
        elif isinstance(definition, dict):
            types = definition.get("type", [])
            for name in [types] if isinstance(types, str) else types:
                if name in classes:
                    yield from fields(classes[name])

    registered = set()
    declared = set()
    for registry in VERIFICATION_REGISTRIES:
        operations = schema.get(registry)
        if isinstance(operations, dict):
            registered.update(operations)
            for definition in operations.values():
                declared.update(fields(definition))
    declared |= registered
    openings = [
        (requirement.id, requirement.text) for requirement in contract.requirements
    ]
    openings += [(card.id, card.statement or "") for card in contract.cards]
    defined = {}
    for identifier, text in openings:
        match = DEFINING_NAME.match(text)
        if match:
            defined.setdefault(match[1], identifier)
    errors = [
        f"{VERIFICATION_CONTRACT}: {identifier} defines `{name}`, "
        "which no verification registry declares"
        for name, identifier in sorted(defined.items())
        if name not in declared
    ]
    errors.extend(
        f"schema_verification.yaml: `{name}` is registered but no "
        f"{VERIFICATION_CONTRACT} rule defines it"
        for name in sorted(registered - defined.keys())
    )
    return errors


# -- the repository -----------------------------------------------------------


def citation_errors(root, rules):
    """Every rule ID cited in the repository names a rule."""
    errors = []
    for path in authored_files(root, CITATION_SUFFIXES, CITATION_NAMES):
        relative = path.relative_to(root).as_posix()
        exempt = CITATION_EXEMPT.get(relative, frozenset())
        if exempt is None:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            continue
        for line, _, identifier in rules.citations(text):
            if identifier not in exempt and rules.resolve(identifier) is None:
                errors.append(
                    f"{relative}:{line}: unresolved rule citation {identifier}"
                )
    for path in sorted((root / "benchmarks").glob("negative-*/expected/error.yaml")):
        try:
            contract = yaml.safe_load(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, yaml.YAMLError):
            continue  # The repository validator reports unreadable fixtures.
        if not isinstance(contract, dict) or contract.get("requirement") is None:
            continue  # Schema-phase failures may carry no requirement.
        cited = contract["requirement"]
        if not isinstance(cited, str) or rules.identifier.fullmatch(cited) is None:
            errors.append(
                f"{path.relative_to(root).as_posix()}: requirement must be a rule ID "
                "(REQ-NNNN or a card ID)"
            )
    return errors


def link_target(root, source, link):
    """The repository path a relative link names, following site pages.

    Returns ``(path, checked)``: ``path`` is None when nothing exists there,
    and ``checked`` is False for generated pages, which have no source file.
    """
    if not link.path:
        return source, True
    joined = posixpath.normpath(posixpath.join(posixpath.dirname(source), link.path))
    if joined.startswith("../") or joined == "..":
        return None, True
    if (root / joined).exists():
        return joined, True
    if source.startswith("docs/"):
        page = joined.removeprefix("docs/")
        if page in CANONICAL:
            return CANONICAL[page], True
        if page in GENERATED or page.startswith(GENERATED_DIRECTORIES):
            return joined, False
    return None, True


def link_errors(root, rules):
    """Relative links name an existing file and anchor; rule ID links agree."""
    errors = []
    cache = {}
    for path in authored_files(root, {".md"}):
        source = path.relative_to(root).as_posix()
        if source in LINK_EXEMPT:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            continue  # The ASCII source check reports undecodable files.
        for link in markdown_links(text):
            if link.external or not link.target:
                continue
            where = f"{source}:{link.line}"
            target, checked = link_target(root, source, link)
            if target is None:
                errors.append(f"{where}: link target {link.target} does not exist")
                continue
            if not checked:
                continue
            fragment = link.fragment
            if fragment and target.endswith(".md"):
                if target not in cache:
                    cache[target] = anchors((root / target).read_text(encoding="utf-8"))
                if fragment not in cache[target]:
                    errors.append(f"{where}: {target} has no anchor #{fragment}")
            label = link.text.strip()
            if rules.identifier.fullmatch(label) is None:
                continue
            location = rules.location(label)
            if location is None:
                continue  # Reported as an unresolved citation.
            expected = f"rules/{location[0]}"
            if (target, fragment) != (expected, location[1]):
                relative = posixpath.relpath(expected, posixpath.dirname(source))
                errors.append(
                    f"{where}: [{label}] links to {link.target}, "
                    f"not {relative}#{location[1]}"
                )
    return errors


def schema_errors(root, rules):
    """Schema comments point at a rule; descriptions cite rules that resolve."""
    errors = []
    for path in sorted((root / "yaml").glob("schema*.yaml")):
        text = path.read_text(encoding="utf-8")
        for comment in SCHEMA_COMMENT.findall(text):
            pointer = POINTER.fullmatch(comment)
            if pointer is None or rules.resolve(pointer["id"]) is None:
                errors.append(f"schema comment has no canonical owner: {path.name}")
        try:
            document = load_schema(path)
        except SchemaSourceError as exc:
            errors.append(str(exc))
            continue
        for field, value in schema_descriptions(document):
            try:
                description_rules(f"{path.name}:{field}", value, rules)
            except SchemaSourceError as exc:
                errors.append(str(exc))
    return errors


def rationale_errors(root):
    """Rules prose, outside code, has no rationale phrases."""
    errors = []
    directory = root / "rules"
    for path in sorted(directory.rglob("*.md")):
        label = path.relative_to(directory).as_posix()
        if label in GENERATED_PAGES:
            continue
        allowed = RATIONALE_ALLOWLIST.get(label, [])
        text = path.read_text(encoding="utf-8")
        for number, line in enumerate(prose_lines(text), 1):
            match = RATIONALE.search(line)
            if match is None or any(re.search(pattern, line) for pattern, _ in allowed):
                continue
            errors.append(
                f"{label}:{number}: rationale phrase '{match[0]}': {' '.join(line.split())}"
            )
    return errors


def unnamed_conditions(rules, conditions):
    """Registered conditions that no card's Fails line or legacy text names."""
    named = {name for card in rules.all_cards() for name in card.fails}
    for requirement in rules.all_requirements():
        named.update(re.findall(r"[a-z][a-z0-9_]*", requirement.text))
    return sorted(set(conditions or ()) - named)


def check(root):
    """Run every rule check once on a checkout."""
    root = Path(root)
    rules = load(root)
    conditions = registered_conditions(root)
    errors = [
        *index_errors(rules),
        *principle_errors(rules),
        *file_errors(rules),
        *card_errors(root, rules, conditions),
        *requirement_errors(rules),
        *verification_errors(root, rules),
        *citation_errors(root, rules),
        *link_errors(root, rules),
        *schema_errors(root, rules),
        *rationale_errors(root),
    ]
    return Report(rules, errors, unnamed_conditions(rules, conditions))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    report = check(parser.parse_args().root)
    if report.unnamed_conditions:
        print(
            f"NOTE: {len(report.unnamed_conditions)} registered conditions are named "
            "by no card Fails line or legacy requirement:"
        )
        print(
            textwrap.fill(
                ", ".join(report.unnamed_conditions),
                width=88,
                initial_indent="NOTE:   ",
                subsequent_indent="NOTE:   ",
                break_on_hyphens=False,
            )
        )
    for error in report.errors:
        print(f"ERROR: {error}")
    if report.errors:
        print(f"FAIL: {len(report.errors)} rule check error(s).")
        return 1
    rules = report.rules
    print(
        f"PASS: {len(rules.cards)} cards and {len(rules.requirements)} legacy "
        "requirements; citations, links, schema navigation and rules prose resolve."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
