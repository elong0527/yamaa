"""Generate the schema field, rule index and rules summary pages.

The pages are views of ``rules/`` and ``yaml/`` rendered on every docs build;
they add no semantics. Their relative links resolve from
``rules/reference/`` and become repository links on the site. The 0.5 schema
fields page reads ``yaml/schema*.yaml`` and the 1.0 page ``yaml/v1/``.
"""

import argparse
import json
import posixpath
from pathlib import Path

from tools.paths import ROOT
from tools.rules.model import (
    RULE_INDEX,
    RULES_SUMMARY,
    SCHEMA_FIELDS,
    SCHEMA_FIELDS_V1,
    edit_lines,
    load,
    markdown_links,
)
from tools.validation.repository_schema import (
    POINTER,
    SchemaSourceError,
    description_rules,
    load_schema,
    schema_descriptions,
)
from tools.validation.schema_bundle import Bundle

PAGE_DIRECTORY = posixpath.dirname(RULE_INDEX)
GENERATED_MARK = "<!-- generated: generate_rule_reference.py -->"
# A link between two generated pages names the site page: a relative link on
# a generated page becomes a repository link, and generated pages are not
# committed.
SCHEMA_FIELDS_SITE = "https://elong0527.github.io/yamaa/reference/schema-fields/"


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
    """Link to the card an ID names."""
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


def schema_fields_v1(root, rules):
    """The 1.0 schema files: each field's shape, kind and default."""
    lines = [
        "# Schema fields (1.0)",
        "",
        GENERATED_MARK,
        "",
        "This table is a generated view of the 1.0 schema files in `yaml/v1/`,",
        "which define the `spec`, `project`, `codelists` and `function` file",
        "types: each field's shape, kind and default. Follow the rule link for",
        "behavior. It is not an additional semantic contract.",
        "",
        "Until the engine reads 1.0 files it reads the 0.5 schema files, shown",
        f"on the [schema fields]({SCHEMA_FIELDS_SITE}) page, and `yaml/README.md`",
        "says how long both sets exist. A field that 1.0 adds or changes links",
        "the rule that will own it.",
        "",
        "A field's kind comes from its type and says what a bare scalar in it is:",
        "",
        "| Kind | A bare scalar is |",
        "| --- | --- |",
        "| `expression` | Text of the closed grammar, in which a bare name is a reference and quoted text a literal; a YAML number, Boolean or null is a literal. |",
        "| `name` | One variable name, bare or qualified. |",
        "| `identifier` | A declared name, or a reference to one. |",
        "| `value` | A constant: text is text, and YAML null is missing where the type admits it. |",
        "| `path` | A local path. |",
        "| `structure` | Nothing: the field holds a class, a list or mapping of classes, or a registry value. |",
        "",
    ]
    directory = root / "yaml" / "v1"
    paths = sorted(directory.glob("schema*.yaml"))
    if not paths:
        return "\n".join(lines)
    bundle = Bundle(directory)
    for path in paths:
        for field, value in schema_descriptions(bundle.modules[path.name]):
            description_rules(f"v1/{path.name}:{field}", value, rules)
    for path in paths:
        lines.extend(
            [
                f"## {path.name}",
                "",
                "| Field or value type | Type | Kind | Required | Default | Constraints | Contract |",
                "| --- | --- | --- | --- | --- | --- | --- |",
            ]
        )
        for name, descriptor in descriptors(
            bundle.modules[path.name], classes=bundle.classes
        ):
            owner, _, rest = name.partition(".")
            # A registry entry's payload is no field, so it has no kind.
            if owner in bundle.registries and "." not in rest:
                kind = "--"
            else:
                try:
                    kind = code_span(bundle.descriptor_kind(descriptor, name))
                except SchemaSourceError as exc:
                    raise ValueError(f"v1/{path.name}: {exc}") from exc
            contract = (
                contract_cell(
                    rules,
                    f"v1/{path.name}:{name}.description",
                    descriptor["description"],
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
                f"| {code_span(name)} | {cell(descriptor['type'])} | {kind} | {cell(descriptor.get('required', False))} | {default} | {cell(constraints) if constraints else '--'} | {contract} |"
            )
        lines.append("")
    return "\n".join(lines)


def rule_index(rules):
    lines = [
        "# Rule index",
        "",
        GENERATED_MARK,
        "",
        "Rule IDs identify rules independently of file paths. A card's ID carries",
        "its file's prefix.",
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
    return "\n".join(lines)


def rules_summary(rules):
    lines = [
        "# Rules at a glance",
        "",
        GENERATED_MARK,
        "",
        "The principles and every rule's title, generated from the",
        f"[rule files]({page_link('README.md')}). Each title states its rule; the",
        "statement a reviewer approves, the detail, conditions and evidence are in",
        "the rule's file.",
        "",
    ]
    if rules.principles:
        lines += ["## Principles", ""]
        for principle in rules.principles:
            text = rebase(principle.text, "README.md")
            lines += [f"**P{principle.number}. {principle.title}** {text}", ""]
    listing = {row.path: row for row in rules.listing or ()}
    for rule_file in rules.ordered_files():
        if not rule_file.cards:
            continue
        lines += [f"## {rule_file.title or rule_file.path}", ""]
        if rule_file.path in listing:
            lines += [listing[rule_file.path].covers, ""]
        topic = None
        for card in rule_file.cards:
            if getattr(card, "topic", None) and card.topic != topic:
                if topic is not None:
                    lines.append("")
                topic = card.topic
                lines += [f"### {topic}", ""]
            target = page_link(card.file, card.id.lower())
            lines.append(f"- [{card.id}]({target}). {card.title}")
        lines.append("")
    return "\n".join(lines)


def generated(root):
    root = Path(root)
    rules = load(root)
    pages = {
        SCHEMA_FIELDS: schema_fields(root, rules),
        SCHEMA_FIELDS_V1: schema_fields_v1(root, rules),
        RULE_INDEX: rule_index(rules),
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
