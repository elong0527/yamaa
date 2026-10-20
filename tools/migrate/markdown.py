"""YAML blocks inside Markdown files.

A fenced `yaml` block is converted in place, indentation kept. A block that
is a whole document (it declares `schema_version`) converts as a file does.
Any other block is a fragment: it is anchored at the schema position it fits
best, the root class, a column list, a derivation, a row template's
derivation map and so on, and converted from there (method page, step 3).
Its intermediates take their keys from the benchmark's entry specification,
the context the README describes.

A fragment carries no version, so it is converted only when it shows a 0.5
spelling: a fragment the rename table leaves unchanged, apart from YAML
nulls, is left as written. That keeps a second run from reading 1.0 text,
such as a `null` that now means missing, under 0.5 rules.

Inline code in the prose that names a converted construct is listed for
manual review; prose is never rewritten.
"""

import re
from collections import Counter
from dataclasses import dataclass

import yaml
from tools.migrate import keys as keys_module
from tools.migrate import schema
from tools.migrate.convert import Converter, Note, Plan
from tools.migrate.text import compose, member, value_of

DRIVER_KEYLESS = "intermediate row-template driver left keyless (D23-R1)"
KEY_UNINFERABLE = "intermediate with no inferable key (D23)"

FENCE = re.compile(r"^(?P<indent>[ \t]*)(?P<fence>`{3,}|~{3,})(?P<info>.*)$")
CODE_SPAN = re.compile(r"`([^`]+)`")
HARNESS = frozenset(
    {
        "lifecycle",
        "standard",
        "domain",
        "entries",
        "covers",
        "conformance",
        "define",
        "harbor",
    }
)
CONTRACT = frozenset(
    {"phase", "condition", "spec_paths", "requirement", "rule", "context"}
)
PROSE_SPELLINGS = re.compile(
    r"\{\s*literal\s*:|\bliteral:|\bcompute:|\{\s*source\s*:|\bschema_version\b"
    r"|\btie_break\b|\bminimum_source_precision\b|\bderive:|\bbounds:"
    r"|\breference_date\b|not_missing:\s*\{\}|environment\.yaml"
)
PREFERRED = (
    ("spec", "root_class"),
    ("project", "project_class"),
    ("environment", "environment_class"),
    ("spec", ("list", "column_class")),
    ("spec", "column_class"),
    ("spec", "derivation"),
    ("spec", ("dict", "identifier", "derivation")),
    # A list of {id, dataset, derivations} fits both; it is a row-template
    # list unless it writes a field only intermediates have.
    ("spec", ("list", "row_class")),
    ("spec", ("list", "intermediate_class")),
    ("spec", ("dict", "identifier", "dataset_source")),
)


@dataclass
class Context:
    """The composed entry specification a README's fragments describe."""

    spec: object
    relations: object


def candidates():
    """Every schema position a fragment may be anchored at, preferred first."""
    found = list(PREFERRED)
    for name in ("spec", "project", "environment"):
        bundle = schema.bundle(name)
        for class_ in bundle.classes.values():
            for descriptor in class_.fields.values():
                item = (name, descriptor.type)
                if item not in found:
                    found.append(item)
        for registry in bundle.registries.values():
            for payload in registry.keywords.values():
                item = (name, payload)
                if item not in found:
                    found.append(item)
    return found


_CANDIDATES = None


def anchor(root):
    """The (bundle, type) a fragment fits best, or None."""
    global _CANDIDATES
    if _CANDIDATES is None:
        _CANDIDATES = candidates()
    best, best_score = None, 0
    for name, type_expr in _CANDIDATES:
        bundle = schema.bundle(name)
        if bundle.shape(type_expr, root) is None:
            continue
        known = unknown = 0
        for visit in bundle.walk(root, type_expr):
            if visit.kind in ("field", "keyword"):
                known += 1
            elif visit.kind == "unknown":
                unknown += 1
        score = known - 3 * unknown
        if score > best_score:
            best, best_score = (name, type_expr), score
    return best


def _fragment_keys(root, anchor_type, context):
    """Today's keys for a fragment's intermediates, read in the README's context."""
    if context is None:
        return {}, 0, []
    if anchor_type == "root_class" and isinstance(root, yaml.MappingNode):
        fragment = value_of(root) or {}
    elif anchor_type == ("list", "intermediate_class"):
        fragment = {"intermediates": value_of(root)}
    elif anchor_type == "intermediate_class":
        fragment = {"intermediates": [value_of(root)]}
    else:
        return {}, 0, []
    intermediates = [
        item for item in fragment.get("intermediates") or [] if isinstance(item, dict)
    ]
    if not intermediates:
        return {}, 0, []
    spec = context.spec
    value = dict(spec.value)
    for name in ("keys", "input", "rows", "columns"):
        if name in fragment:
            value[name] = fragment[name]
    synthetic = keys_module.Composed(
        spec.entry, value, spec.layers, spec.declarers, spec.path_bases
    )
    keys = [str(key) for key in value.get("keys") or []]
    drivers = {
        row.get("dataset") for row in value.get("rows") or [] if isinstance(row, dict)
    }
    found, problems, keyless = {}, [], 0
    for item in intermediates:
        identifier = item.get("id")
        if "key" in item:
            continue
        if identifier in drivers:
            keyless += 1
            continue
        dataset = item.get("dataset")
        if dataset == "SELF":
            fields = [
                column.get("name")
                for column in value.get("columns") or []
                if isinstance(column, dict)
            ]
        else:
            fields, problem = context.relations.fields(synthetic, dataset)
            if fields is None:
                problems.append(f"intermediate {identifier}: {problem}")
                continue
            fields = fields + list((item.get("derivations") or {}).keys())
        applicable = [key for key in keys if key in fields]
        if applicable:
            found[identifier] = applicable
        else:
            problems.append(
                f"intermediate {identifier}: {dataset} carries none of the output keys {keys}"
            )
    return found, keyless, problems


def convert_block(text, context, table, notes, label, line_offset):
    """A converted block, or None when it is left as written; and its changes."""
    try:
        root = compose(text)
    except yaml.YAMLError as error:
        notes.append(
            Note(
                label,
                line_offset + 1,
                "yaml",
                f"YAML block not parsed, left as written: {str(error).splitlines()[0]}",
            )
        )
        return None, Counter()
    if root is None or isinstance(root, yaml.ScalarNode):
        return None, Counter()
    names = set()
    if isinstance(root, yaml.MappingNode):
        names = {key.value for key, _ in root.value if isinstance(key, yaml.ScalarNode)}
    if "yamaa" in names:
        return None, Counter()
    if names and names <= HARNESS and names - {"domain", "standard"}:
        # benchmark.yaml metadata, which the converter never rewrites.
        return None, Counter()
    if names and names <= CONTRACT:
        # A negative contract shown in prose: its spec paths name no file.
        return None, Counter()
    if "schema_version" in names:
        if names & {"environments", "codelists", "study", "sdtm", "adam", "send"}:
            bundle, header_type = "project", "project"
        elif names & {"functions", "lock"}:
            notes.append(
                Note(
                    label,
                    line_offset + 1,
                    "D8",
                    "an environment file in a YAML block; rewrite it as a function file by hand",
                )
            )
            return None, Counter()
        else:
            bundle, header_type = "spec", "spec"
        plan = Plan(
            bundle=bundle,
            layered="parent" in names,
            header_type=header_type,
            label=label,
            line_offset=line_offset,
        )
        if bundle == "project" and "environments" in names:
            notes.append(
                Note(
                    label,
                    line_offset + 1,
                    "D8",
                    "a project block links environment files; write its functions and locks by hand",
                )
            )
        if bundle == "spec" and "parent" in names:
            notes.append(
                Note(
                    label,
                    line_offset + 1,
                    "clear",
                    "a child layer in a YAML block; check that its nulls clear inherited values",
                )
            )
        if bundle == "spec":
            plan.intermediate_keys, keyless, problems = _fragment_keys(
                root, "root_class", context
            )
            plan.changes[DRIVER_KEYLESS] += keyless
            plan.changes[KEY_UNINFERABLE] += len(problems)
            for problem in problems:
                notes.append(
                    Note(
                        label,
                        line_offset + 1,
                        "keys",
                        f"{problem}; declare `key` by hand (D23)",
                    )
                )
        converter = Converter(plan, table, notes)
        converted = converter.convert(text)
        return converted, converter.changes
    found = anchor(root)
    if found is None:
        return None, Counter()
    bundle, anchor_type = found
    plan = Plan(bundle=bundle, anchor=anchor_type, label=label, line_offset=line_offset)
    problems = []
    if bundle == "spec":
        plan.intermediate_keys, keyless, problems = _fragment_keys(
            root, anchor_type, context
        )
        plan.changes[DRIVER_KEYLESS] += keyless
        plan.changes[KEY_UNINFERABLE] += len(problems)
    for problem in problems:
        notes.append(
            Note(
                label,
                line_offset + 1,
                "keys",
                f"{problem}; declare `key` by hand (D23)",
            )
        )
    local = []
    converter = Converter(plan, table, local)
    converted = converter.convert(text)
    if not converter.markers:
        return None, Counter()
    notes.extend(local)
    if member(root, "intermediates") is not None and context is None:
        notes.append(
            Note(
                label,
                line_offset + 1,
                "keys",
                "intermediates in a YAML block with no specification to read keys from",
            )
        )
    return converted, converter.changes


def _widen(context, block):
    """The context with the inputs and keys a root fragment declares, for later blocks."""
    if context is None:
        return None
    try:
        value = value_of(compose(block))
    except yaml.YAMLError:
        return context
    if not isinstance(value, dict) or not (set(value) & {"input", "keys"}):
        return context
    spec = context.spec
    widened = dict(spec.value)
    if isinstance(value.get("input"), dict):
        widened["input"] = {**(widened.get("input") or {}), **value["input"]}
    if isinstance(value.get("keys"), list):
        widened["keys"] = value["keys"]
    composed = keys_module.Composed(
        spec.entry, widened, spec.layers, spec.declarers, spec.path_bases
    )
    return Context(composed, context.relations)


def blocks(lines):
    """(first content line, end line, indent, info) of every fenced block."""
    index = 0
    while index < len(lines):
        match = FENCE.match(lines[index])
        if not match:
            index += 1
            continue
        fence = match.group("fence")
        indent = match.group("indent")
        info = (
            match.group("info").strip().split(" ", 1)[0].lower()
            if match.group("info").strip()
            else ""
        )
        close = index + 1
        while close < len(lines):
            stripped = lines[close].strip()
            if stripped and set(stripped) == {fence[0]} and len(stripped) >= len(fence):
                break
            close += 1
        yield index + 1, close, indent, info
        index = close + 1


def convert_markdown(text, context, table, notes, label):
    """A Markdown file with its YAML blocks converted; the changes of each block."""
    newline = "\r\n" if "\r\n" in text else "\n"
    lines = text.split(newline)
    out = list(lines)
    changed = []
    covered = set()
    replacements = []
    for first, close, indent, info in blocks(lines):
        covered.update(range(first - 1, close + 1))
        if info not in ("yaml", "yml"):
            continue
        body = lines[first:close]
        width = len(indent)
        dedented = [
            line[width:] if line[:width].strip() == "" else line.lstrip()
            for line in body
        ]
        block = "\n".join(dedented) + "\n"
        converted, changes = convert_block(block, context, table, notes, label, first)
        context = _widen(context, block)
        if converted is None or converted == block:
            continue
        new_lines = converted.split("\n")
        if new_lines and new_lines[-1] == "":
            new_lines.pop()
        replacements.append(
            (first, close, [indent + line if line else line for line in new_lines])
        )
        changed.append((f"{label}:{first + 1}", changes))
    for first, close, new_lines in reversed(replacements):
        out[first:close] = new_lines
    for number, line in enumerate(lines):
        if number in covered:
            continue
        for match in CODE_SPAN.finditer(line):
            if PROSE_SPELLINGS.search(match.group(1)):
                notes.append(
                    Note(
                        label,
                        number + 1,
                        "prose",
                        f"prose names a converted construct: `{match.group(1)}`",
                    )
                )
    return newline.join(out), changed
