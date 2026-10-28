"""The file-type schemas and grammars of the interface stability check (#3030
P-STAB-1, manifest 1).

`compare` diffs the working tree's copies with the baseline release tag's
(`tools/validation/check_interface_stability.py`).

The JSON Schemas under JSON_SCHEMAS, one per file type, which
tools/packaging/generate_json_schema.py writes from the schema files. Each one
is normalized into an inventory of elements, named as tools/migrate/renames.yaml
names fields: the root `$ref`; each named type of `$defs`; each field of a class,
`<class>.<field>`; each keyword of a registry, `<registry>.<keyword>`; and each
field of a keyword's payload, `<registry>.<keyword>.<field>`. An element's
facts are its type, whether it is required, its default, its enumerated values,
its field kind (`x-yamaa-kind`) and its extra-property rule. Its type is the
set of alternatives it admits, each a JSON type, a named type by name, or a
list or dict of alternatives, together with its pattern and length bounds.
These break:

- a removed element, so a renamed field or class is a removed element and an
  added one;
- a narrowed type: an alternative no longer admitted, a new pattern, a raised
  lower or a lowered upper bound, or a changed pattern, since the check cannot
  tell whether it admits more;
- a changed field kind or default, a default added or removed included;
- a new required field of an element the baseline has, or a field made
  required;
- a removed enumerated value, or enumerated values where there were none;
- extra properties refused where they were admitted, or a changed schema
  restricting their values, conservatively unless they were already refused.

Descriptions and the notation type (`x-yamaa-type`) are not compared, and any
change to a keyword not named here breaks, including at the schema root.
Unsupported conjunctions of type keywords fail the check.

The grammars, yaml/grammar/expression.yaml and
yaml/grammar/string-template.yaml. Each one is normalized into its functions,
with their argument bounds and result; its reducers, with their argument forms;
its keywords, each word a production quotes; its operators, each other quoted
terminal except the bounds of a range; its productions, by name; and its
reserved words. A removed element breaks, as do a function whose bounds narrow
or whose result changes and a reducer that loses an argument form. A reserved
word works the other way round: a new one refuses a bare identifier that an
earlier release admitted, so it breaks, and a removed one is an addition.
Production definitions and the vectors are not compared.

Every other difference is an addition. A manifest the baseline lacks is new,
so each of its elements is an addition. An element whose owner is added or
removed is not listed again. The working tree must hold both grammars and at
least one JSON Schema.
"""

import json
import re
from pathlib import PurePosixPath

import yaml
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.interface_manifests import Change, ManifestError

# Where the JSON Schemas are committed. From 1.0.0 they keep this path (#3030
# D161), so the baseline tag holds them here too.
JSON_SCHEMAS = "yaml/json"
SCHEMA_SUFFIX = ".schema.json"
EXPRESSION = "yaml/grammar/expression.yaml"
TEMPLATE = "yaml/grammar/string-template.yaml"
GRAMMARS = (EXPRESSION, TEMPLATE)

DEFINITIONS = "#/$defs/"
# The keywords a node's type is read from, and those never compared.
TYPE_KEYWORDS = {
    "$ref",
    "anyOf",
    "type",
    "items",
    "propertyNames",
    "additionalProperties",
}
PROSE = {"description", "title", "x-yamaa-type"}
# A lower bound narrows the type when it rises, an upper bound when it falls.
LOWER_BOUNDS = ("minLength", "minItems", "minProperties")
UPPER_BOUNDS = ("maxLength", "maxItems", "maxProperties")
# Every keyword an element's facts name; a change to any other one breaks.
FACTS = {"properties", "required", "default", "enum", "pattern", "x-yamaa-kind"}
READ = TYPE_KEYWORDS | PROSE | FACTS | {*LOWER_BOUNDS, *UPPER_BOUNDS}

TERMINAL = re.compile(r'"([^"]*)"')
RANGE = re.compile(r'"[^"]*"\s*\.\.\.\s*"[^"]*"')
WORD = re.compile(r"[A-Za-z]+")


def compare(baseline, current):
    """Every change of the JSON Schemas and the grammars from `baseline` to
    `current`."""
    return [
        *_compare_schemas(baseline, current),
        *_compare_grammars(baseline, current),
    ]


def _canonical(value):
    return json.dumps(value, sort_keys=True)


def _schema_without_prose(node):
    """Drop annotations at schema nodes, preserving names and literal data."""
    if not isinstance(node, dict):
        return node
    result = {}
    for keyword, value in node.items():
        if keyword in PROSE:
            continue
        if keyword in {"$defs", "properties", "patternProperties", "dependentSchemas"}:
            if isinstance(value, dict):
                value = {
                    name: _schema_without_prose(schema)
                    for name, schema in value.items()
                }
        elif keyword in {"allOf", "anyOf", "oneOf", "prefixItems"}:
            if isinstance(value, list):
                value = [_schema_without_prose(schema) for schema in value]
        elif keyword in {
            "items",
            "additionalProperties",
            "unevaluatedProperties",
            "unevaluatedItems",
            "propertyNames",
            "contains",
            "contentSchema",
            "not",
            "if",
            "then",
            "else",
        }:
            value = _schema_without_prose(value)
        result[keyword] = value
    return result


def _shown(value):
    return "none" if value is None else f"`{value}`"


def _changed(fact, old, new):
    return f"{fact} changed from {_shown(old)} to {_shown(new)}"


def _quoted(values):
    return ", ".join(f"`{value}`" for value in values)


def _many(noun, values, verb):
    return f"{noun}{'' if len(values) == 1 else 's'} {_quoted(values)} {verb}"


def schema_paths(tree):
    """The JSON Schemas directly under JSON_SCHEMAS in `tree`."""
    directory = PurePosixPath(JSON_SCHEMAS)
    return [
        path
        for path in tree.files(JSON_SCHEMAS)
        if path.endswith(SCHEMA_SUFFIX) and PurePosixPath(path).parent == directory
    ]


def _alternatives(node, where, inner=False):
    """The alternatives a schema node admits: the name of a JSON type or a
    named type, ("list", items) or ("dict", keys, values)."""
    if not isinstance(node, dict):
        raise ManifestError(f"{where}: expected a schema object")
    unread = sorted(set(node) - TYPE_KEYWORDS - PROSE) if inner else []
    if unread:
        raise ManifestError(f"{where}: cannot compare {', '.join(unread)} in a type")
    for keyword in ("$ref", "anyOf"):
        siblings = sorted(
            (set(node) & (TYPE_KEYWORDS | {"properties", "required"})) - {keyword}
        )
        if keyword in node and siblings:
            raise ManifestError(
                f"{where}: cannot compare {', '.join(siblings)} alongside {keyword}"
            )
    if "$ref" in node:
        reference = node["$ref"]
        if not (isinstance(reference, str) and reference.startswith(DEFINITIONS)):
            raise ManifestError(f"{where}: expected a reference into $defs")
        return frozenset([reference.removeprefix(DEFINITIONS)])
    if "anyOf" in node:
        if not (isinstance(node["anyOf"], list) and node["anyOf"]):
            raise ManifestError(f"{where}: expected anyOf to list schemas")
        return frozenset().union(
            *(_alternatives(member, where, True) for member in node["anyOf"])
        )
    names = node.get("type")
    if names is None and {"if", "then", "else"} <= node.keys():
        # A conditional wrapper admits types from either branch. Its
        # predicate and both branches remain conservative constraints in
        # `other`, so changing any of them is still reported as breaking.
        return _alternatives(node["then"], where, True) | _alternatives(
            node["else"], where, True
        )
    names = [names] if isinstance(names, str) else names
    if not (
        isinstance(names, list)
        and names
        and all(isinstance(name, str) for name in names)
    ):
        raise ManifestError(f"{where}: expected a type, a reference or anyOf")
    alternatives = set()
    for name in names:
        if name == "array":
            alternatives.add(("list", _alternatives(node.get("items"), where, True)))
        elif name == "object" and "properties" not in node:
            keys = _alternatives(
                node.get("propertyNames", {"type": "string"}), where, True
            )
            values = _alternatives(node.get("additionalProperties"), where, True)
            alternatives.add(("dict", keys, values))
        else:
            alternatives.add(name)
    return frozenset(alternatives)


def _render(alternatives):
    """Alternatives as text, such as `list[identifier] | null`."""
    return " | ".join(sorted(map(_render_one, alternatives)))


def _render_one(alternative):
    if isinstance(alternative, str):
        return alternative
    return f"{alternative[0]}[{', '.join(map(_render, alternative[1:]))}]"


def _admits(after, before):
    """Whether the alternatives `after` admit every value `before` admits."""
    return all(any(_covers(new, old) for new in after) for old in before)


def _covers(new, old):
    if isinstance(new, tuple) and isinstance(old, tuple) and new[0] == old[0]:
        return all(map(_admits, new[1:], old[1:]))
    return new == old


def _facts(node, where, parent, required):
    node = _schema_without_prose(node)
    values = node.get("enum")
    bounds = {bound: node.get(bound) for bound in LOWER_BOUNDS + UPPER_BOUNDS}
    if not (values is None or isinstance(values, list)) or not all(
        value is None or (type(value) is int and value >= 0)
        for value in bounds.values()
    ):
        raise ManifestError(f"{where}: expected a list of values and integer bounds")
    extra = None
    if "properties" in node:
        rule = node.get("additionalProperties", True)
        if type(rule) is bool:
            extra = "open" if rule else "closed"
        elif isinstance(rule, dict):
            extra = _canonical(rule) if rule else "open"
        else:
            raise ManifestError(
                f"{where}: expected an extra-property schema or boolean"
            )
    return {
        "parent": parent,
        "type": _alternatives(node, where),
        "required": required,
        "kind": node.get("x-yamaa-kind"),
        "default": _canonical(node["default"]) if "default" in node else None,
        "enum": None if values is None else [_canonical(value) for value in values],
        "extra": extra,
        "pattern": node.get("pattern"),
        **bounds,
        "other": {
            keyword: _canonical(value)
            for keyword, value in node.items()
            if keyword not in READ
            or (keyword == "propertyNames" and "properties" in node)
        },
    }


def schema_inventory(tree, path):
    """Map each element of the JSON Schema at `path` to its facts, in the
    order the file lists them; empty when the tree has no such file."""
    text = tree.text(path)
    if text is None:
        return {}
    try:
        document = json.loads(text)
    except ValueError as error:
        raise ManifestError(f"{path} at {tree}: {error}") from None
    definitions = document.get("$defs") if isinstance(document, dict) else None
    if not isinstance(definitions, dict):
        raise ManifestError(
            f"{path} at {tree}: expected `$defs`, mapping each named type to its schema"
        )
    inventory = {}

    def add(element, node, parent=None, required=False):
        where = f"{path} at {tree}: {element}"
        if element in inventory:
            raise ManifestError(f"{where}: named twice")
        if not isinstance(node, dict):
            raise ManifestError(f"{where}: expected a schema object")
        inventory[element] = _facts(node, where, parent, required)
        properties, needed = node.get("properties", {}), node.get("required", [])
        if not (isinstance(properties, dict) and isinstance(needed, list)):
            raise ManifestError(f"{where}: expected properties and their names")
        if any(not isinstance(name, str) or name not in properties for name in needed):
            raise ManifestError(f"{where}: required names must have property schemas")
        for name, field in properties.items():
            add(f"{element}.{name}", field, element, name in needed)

    root = {key: value for key, value in document.items() if key != "$defs"}
    root.setdefault("$ref", None)
    add("$ref", root)
    for name, node in definitions.items():
        add(name, node)
    return inventory


def _compare_schemas(baseline, current):
    paths = schema_paths(current)
    if not paths:
        raise ManifestError(f"{JSON_SCHEMAS}: no JSON Schema in {current}")
    for path in sorted({*schema_paths(baseline), *paths}):
        before = schema_inventory(baseline, path)
        after = schema_inventory(current, path)
        for element, old in before.items():
            if element in after:
                yield from _compare_facts(path, element, old, after[element])
            elif old["parent"] is None or old["parent"] in after:
                yield Change(path, element, "removed", breaking=True)
        for element, new in after.items():
            if element not in before and (
                new["parent"] is None or new["parent"] in before
            ):
                # A new required field of an element the baseline has breaks.
                what = "added, required" if new["required"] else "added"
                yield Change(path, element, what, breaking=new["required"])


def _compare_facts(manifest, element, old, new):
    def change(what, breaking):
        return Change(manifest, element, what, breaking)

    narrowed = not _admits(new["type"], old["type"])
    widened = not _admits(old["type"], new["type"])
    if narrowed or widened:
        verb = (
            "changed" if narrowed and widened else "narrowed" if narrowed else "widened"
        )
        types = f"`{_render(old['type'])}` to `{_render(new['type'])}`"
        yield change(f"type {verb} from {types}", narrowed)
    if old["required"] != new["required"]:
        yield change(
            "now required" if new["required"] else "now optional", new["required"]
        )
    for fact in ("kind", "default"):
        if old[fact] != new[fact]:
            yield change(_changed(fact, old[fact], new[fact]), True)
    if old["enum"] != new["enum"]:
        if new["enum"] is None:
            yield change("no longer restricted to enumerated values", False)
        elif old["enum"] is None:
            values = _quoted(new["enum"])
            yield change(f"now restricted to enumerated values {values}", True)
        else:
            if lost := [value for value in old["enum"] if value not in new["enum"]]:
                yield change(_many("enumerated value", lost, "removed"), True)
            if gained := [value for value in new["enum"] if value not in old["enum"]]:
                yield change(_many("enumerated value", gained, "added"), False)
    if None not in (old["extra"], new["extra"]) and old["extra"] != new["extra"]:
        if new["extra"] in ("open", "closed"):
            closed = new["extra"] == "closed"
            what = "extra properties" + (" now refused" if closed else " now admitted")
            yield change(what, closed)
        else:
            # A schema can restrict the values of extra properties. Except
            # when extras were already forbidden, conservatively report a
            # changed schema as breaking rather than treating it as open.
            yield change(
                _changed("extra-property rule", old["extra"], new["extra"]),
                old["extra"] != "closed",
            )
    for bound in LOWER_BOUNDS + UPPER_BOUNDS:
        before, after = old[bound], new[bound]
        if before != after:
            if bound in LOWER_BOUNDS:
                narrows = (after or 0) > (before or 0)
            else:
                narrows = after is not None and (before is None or after < before)
            yield change(_changed(bound, before, after), narrows)
    if old["pattern"] != new["pattern"]:
        what = _changed("pattern", old["pattern"], new["pattern"])
        yield change(what, new["pattern"] is not None)
    for keyword in sorted({*old["other"], *new["other"]}):
        before, after = old["other"].get(keyword), new["other"].get(keyword)
        if before != after:
            yield change(_changed(keyword, before, after), True)


def grammar_inventory(tree, path):
    """Map each element of the grammar at `path` to its facts: a function's
    argument bounds and result, a reducer's argument forms, and None for
    every other element. None when the tree has no such file."""
    text = tree.text(path)
    if text is None:
        return None
    try:
        document = yaml.load(text, Loader=UniqueKeyLoader)
    except (yaml.YAMLError, ValueError) as error:
        raise ManifestError(f"{path} at {tree}: {error}") from None
    malformed = ManifestError(
        f"{path} at {tree}: expected productions, vocabulary and reserved words "
        "as yaml/grammar/README.md states"
    )
    if not isinstance(document, dict):
        raise malformed
    productions = document.get("productions")
    vocabulary = document.get("vocabulary", {})
    reserved = document.get("reserved", [])
    if not (
        isinstance(productions, list)
        and isinstance(vocabulary, dict)
        and isinstance(reserved, list)
    ):
        raise malformed
    functions = vocabulary.get("function", {})
    reducers = vocabulary.get("reducer", {})
    if not (
        all(
            isinstance(production, dict)
            and isinstance(production.get("name"), str)
            and isinstance(production.get("definition"), str)
            for production in productions
        )
        and isinstance(functions, dict)
        and all(
            isinstance(declaration, dict)
            and type(declaration.get("min_arguments")) is int
            and type(declaration.get("max_arguments")) in (int, type(None))
            and isinstance(declaration.get("result"), str)
            for declaration in functions.values()
        )
        and isinstance(reducers, dict)
        and all(
            isinstance(declaration, dict)
            and isinstance(declaration.get("argument"), list)
            and all(isinstance(form, str) for form in declaration["argument"])
            for declaration in reducers.values()
        )
        and all(isinstance(word, str) for word in reserved)
    ):
        raise malformed
    # A function's bounds and result; no upper bound is None.
    inventory = {
        f"function {name}": (
            declaration["min_arguments"],
            declaration.get("max_arguments"),
            declaration["result"],
        )
        for name, declaration in functions.items()
    }
    inventory.update(
        (f"reducer {name}", tuple(declaration["argument"]))
        for name, declaration in reducers.items()
    )
    keywords, operators = {}, {}
    for production in productions:
        if production.get("prose") is not True:
            definition = RANGE.sub(" ", production["definition"])
            for terminal in TERMINAL.findall(definition):
                (keywords if WORD.fullmatch(terminal) else operators)[terminal] = None
    inventory.update((f'keyword "{terminal}"', None) for terminal in keywords)
    inventory.update((f'operator "{terminal}"', None) for terminal in operators)
    inventory.update((f"production {item['name']}", None) for item in productions)
    inventory.update((f'reserved "{word}"', None) for word in reserved)
    return inventory


def _reserved(element):
    """Whether a grammar element is a reserved word, which a bare identifier
    may not take: adding one breaks, and removing one does not."""
    return element.startswith('reserved "')


def _arguments(lower, upper):
    if upper == lower:
        return f"{lower} argument{'' if lower == 1 else 's'}"
    if upper is None:
        return f"{lower} or more arguments"
    return f"{lower} to {upper} arguments"


def _compare_grammars(baseline, current):
    for path in GRAMMARS:
        after = grammar_inventory(current, path)
        if after is None:
            raise ManifestError(f"{path}: missing from {current}")
        before = grammar_inventory(baseline, path)
        if before is None:
            # A new grammar admitted no text before, so it refuses none.
            for element in after:
                yield Change(path, element, "added", breaking=False)
            continue
        for element, old in before.items():
            if element not in after:
                if _reserved(element):
                    what = "removed: a bare identifier may take it"
                    yield Change(path, element, what, breaking=False)
                else:
                    yield Change(path, element, "removed", breaking=True)
            elif old != after[element]:
                yield from _compare_declarations(path, element, old, after[element])
        for element in after:
            if element not in before:
                if _reserved(element):
                    what = "added: a bare identifier may no longer take it"
                    yield Change(path, element, what, breaking=True)
                else:
                    yield Change(path, element, "added", breaking=False)


def _compare_declarations(path, element, old, new):
    """A function's or a reducer's changed declaration."""
    if element.startswith("reducer "):
        if lost := [form for form in old if form not in new]:
            yield Change(path, element, _many("argument form", lost, "removed"), True)
        if gained := [form for form in new if form not in old]:
            yield Change(path, element, _many("argument form", gained, "added"), False)
        return
    (lower, upper, result), (new_lower, new_upper, new_result) = old, new
    narrowed = (
        new_lower > lower
        or (new_upper is not None and (upper is None or new_upper < upper))
        or new_result != result
    )
    what = (
        f"changed from `{_arguments(lower, upper)}, {result}` to "
        f"`{_arguments(new_lower, new_upper)}, {new_result}`"
    )
    yield Change(path, element, what, narrowed)
