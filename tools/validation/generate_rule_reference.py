"""Generate the schema field, requirement index and rules summary pages.

The pages are views of ``rules/`` and ``yaml/`` rendered on every docs build;
they add no semantics. Their relative links resolve from
``rules/reference/`` and become repository links on the site.
"""

import argparse
import json
import posixpath
from pathlib import Path

from tools.paths import ROOT
from tools.rules.model import (
    REQUIREMENT_INDEX,
    RULES_SUMMARY,
    SCHEMA_FIELDS,
    edit_lines,
    load,
    markdown_links,
)
from tools.validation.repository_schema import (
    POINTER,
    description_rules,
    load_schema,
    schema_descriptions,
)

PAGE_DIRECTORY = posixpath.dirname(REQUIREMENT_INDEX)
GENERATED_MARK = "<!-- generated: generate_rule_reference.py -->"


def descriptors(node, parts=(), classes=None, active=()):
    if isinstance(node, dict):
        if "type" in node and not isinstance(node["type"], dict):
            yield ".".join(parts), node
        for key, value in node.items():
            if isinstance(value, (dict, list)):
                yield from descriptors(value, (*parts, key), classes, active)
    elif isinstance(node, list):
        for value in node:
            if isinstance(value, dict) and "fields_from" in value:
                name = value["fields_from"]
                if not isinstance(name, str) or classes is None or name not in classes:
                    raise ValueError(f"unknown fields_from class {name!r}")
                if name in active:
                    raise ValueError(f"class fields_from cycle at {name!r}")
                yield from descriptors(classes[name], parts, classes, (*active, name))
            else:
                yield from descriptors(value, parts, classes, active)


def code_span(text):
    # A code span's fence must be longer than any backtick run inside it,
    # or the first embedded backtick ends the span and corrupts the table.
    fence = "`"
    while fence in text:
        fence += "`"
    return fence + text + fence


def cell(value):
    return code_span(json.dumps(value, ensure_ascii=True).replace("|", "\\|"))


def table_text(text):
    """Markdown prose as one table cell."""
    return " ".join(text.split()).replace("|", "\\|")


def page_link(path, fragment=""):
    """A link target from a generated page to a rules-relative path."""
    target = posixpath.relpath(path, PAGE_DIRECTORY)
    return f"{target}#{fragment}" if fragment else target


def rule_link(rules, identifier):
    """Link to the rule an ID names; a replaced ID links to its card."""
    rule = rules.resolve(identifier)
    return f"[{rule.id}]({page_link(rule.file, rule.id.lower())})"


def rebase(text, source):
    """Re-point a rules file's relative links so they resolve from the pages."""
    edits = []
    for link in markdown_links(text):
        if link.external or not link.target:
            continue
        path = posixpath.normpath(posixpath.join(posixpath.dirname(source), link.path))
        target = page_link(path if link.path else source)
        if query := link.target.split("#", 1)[0].partition("?")[2]:
            target += f"?{query}"
        if link.fragment:
            target += f"#{link.fragment}"
        edits.append((link.line, link.target_start, link.target_end, target))
    return edit_lines(text, edits)


def contract_cell(rules, source, value):
    """The schema field table's Contract cell for one description."""
    cited = description_rules(source, value, rules)
    if POINTER.fullmatch(value):
        return rule_link(rules, cited[0])
    return table_text(
        rules.citation.sub(lambda match: rule_link(rules, match[0]), value)
    )


def schema_fields(root, rules):
    lines = [
        "# Schema fields",
        "",
        GENERATED_MARK,
        "",
        "This table is a generated view of schema shape and defaults. Follow the",
        "rule link for behavior. It is not an additional semantic contract.",
        "",
    ]
    documents = {
        path: load_schema(path) for path in sorted((root / "yaml").glob("schema*.yaml"))
    }
    for path, document in documents.items():
        for field, value in schema_descriptions(document):
            description_rules(f"{path.name}:{field}", value, rules)
    classes = {
        name: definition
        for document in documents.values()
        for name, definition in document.items()
        if isinstance(definition, list)
    }
    for path, document in documents.items():
        lines.extend(
            [
                f"## {path.name}",
                "",
                "| Field or value type | Type | Required | Default | Constraints | Contract |",
                "| --- | --- | --- | --- | --- | --- |",
            ]
        )
        for name, descriptor in descriptors(document, classes=classes):
            contract = (
                contract_cell(
                    rules, f"{path.name}:{name}.description", descriptor["description"]
                )
                if "description" in descriptor
                else "Schema constraint"
            )
            constraints = {
                k: descriptor[k]
                for k in ("values", "pattern", "min_length", "size")
                if k in descriptor
            }
            default = (
                cell(descriptor["default"]) if "default" in descriptor else "Absent"
            )
            lines.append(
                f"| {code_span(name)} | {cell(descriptor['type'])} | {cell(descriptor.get('required', False))} | {default} | {cell(constraints) if constraints else '--'} | {contract} |"
            )
        lines.append("")
    return "\n".join(lines)


def requirement_index(rules):
    lines = [
        "# Requirement index",
        "",
        GENERATED_MARK,
        "",
        "Rule IDs identify rules independently of file paths. A card's ID carries",
        "its file's prefix; a contract not yet converted to cards keeps its",
        "`REQ-NNNN` requirements.",
        "",
    ]
    cards = [card for rule_file in rules.ordered_files() for card in rule_file.cards]
    if cards:
        lines += ["## Rule cards", "", "| Rule | Title | File |", "| --- | --- | --- |"]
        for card in cards:
            lines.append(
                f"| {rule_link(rules, card.id)} | {table_text(card.title)} "
                f"| `{card.file}` |"
            )
        lines.append("")
    if rules.requirements:
        lines += [
            "## Requirements not yet converted",
            "",
            "| Requirement | Contract |",
            "| --- | --- |",
        ]
        for identifier, requirement in sorted(rules.requirements.items()):
            lines.append(f"| {rule_link(rules, identifier)} | `{requirement.file}` |")
        lines.append("")
    replaced = sorted(set(rules.replaced) - set(rules.requirements))
    if replaced:
        lines += [
            "## Replaced requirements",
            "",
            "Until the conversion completes, each of these IDs resolves to the",
            "card that replaces it.",
            "",
            "| Requirement | Card |",
            "| --- | --- |",
        ]
        for identifier in replaced:
            lines.append(f"| {identifier} | {rule_link(rules, identifier)} |")
        lines.append("")
    return "\n".join(lines)


def rules_summary(rules):
    lines = [
        "# Rules at a glance",
        "",
        GENERATED_MARK,
        "",
        "The principles and each rule's title and statement, generated from the",
        f"[rule files]({page_link('README.md')}). A rule's detail, conditions and",
        "evidence are in its file.",
        "",
    ]
    if rules.principles:
        lines += ["## Principles", ""]
        for principle in rules.principles:
            text = rebase(principle.text, "README.md")
            lines += [f"**P{principle.number}. {principle.title}** {text}", ""]
    listing = {row.path: row for row in rules.listing or ()}
    legacy = []
    for rule_file in rules.ordered_files():
        if not rule_file.cards:
            legacy.append(rule_file)
            continue
        lines += [f"## {rule_file.title or rule_file.path}", ""]
        if rule_file.path in listing:
            lines += [listing[rule_file.path].covers, ""]
        for card in rule_file.cards:
            target = page_link(card.file, card.id.lower())
            lines += [f"### [{card.id}]({target}). {card.title}", ""]
            if card.statement:
                lines += [rebase(card.statement, card.file), ""]
    if legacy:
        lines += [
            "## Not yet converted",
            "",
            "These contracts keep their `REQ-NNNN` requirements until they become",
            "cards.",
            "",
        ]
        for rule_file in legacy:
            covers = listing[rule_file.path].covers if rule_file.path in listing else ""
            title = rule_file.title or rule_file.path
            entry = f"- [{title}]({page_link(rule_file.path)})"
            lines.append(f"{entry}: {covers}" if covers else entry)
        lines.append("")
    return "\n".join(lines)


def generated(root):
    root = Path(root)
    rules = load(root)
    pages = {
        SCHEMA_FIELDS: schema_fields(root, rules),
        REQUIREMENT_INDEX: requirement_index(rules),
        RULES_SUMMARY: rules_summary(rules),
    }
    return {root / "rules" / page: text for page, text in pages.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale = []
    try:
        references = generated(args.root)
    except ValueError as exc:
        print(f"ERROR: {exc}")
        return 1
    for path, text in references.items():
        if args.check:
            # The references are generated at docs build time and are not
            # committed; an absent file is current by construction. A file
            # materialized by a local build must still match its sources.
            if path.exists():
                try:
                    current = path.read_text(encoding="ascii")
                except UnicodeDecodeError:
                    # The generated text is pure ASCII, so a materialized
                    # reference with non-ASCII bytes is stale by definition;
                    # report it instead of dying with a traceback.
                    current = None
                if current != text:
                    stale.append(str(path.relative_to(args.root)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="ascii")
    for path in stale:
        print(f"ERROR: regenerate {path}")
    return int(bool(stale))


if __name__ == "__main__":
    raise SystemExit(main())
