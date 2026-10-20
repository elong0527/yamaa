"""Today's schema bundles as a type model the converter walks documents with.

The rename table locates every entry by today's schema position,
`yaml/<file>#<owner>.<field>`, where an owner is a named class or a registry
payload such as `expressions.diff`. This module reads the three 0.5 bundles
under `yaml/` as SCHEMA-1 to SCHEMA-10 describe them (classes, `fields_from`,
registries and unions) and walks a document node by node, attributing each
mapping key to the class field or registry keyword that admits it.
"""

import re
from dataclasses import dataclass, field
from pathlib import Path

import yaml
from tools.migrate.text import core_type
from tools.paths import ROOT

SCHEMA_DIR = ROOT / "yaml"
SCALARS = frozenset({"str", "int", "float", "bool", "null"})
BUNDLES = {
    "spec": ("schema.yaml", "root_class"),
    "project": ("schema_project.yaml", "project_class"),
    "environment": ("schema_environment.yaml", "environment_class"),
}


@dataclass(frozen=True)
class Field:
    name: str
    type: object
    required: bool = False
    default: object = None
    values: tuple = ()


@dataclass
class Class:
    name: str
    fields: dict = field(default_factory=dict)


@dataclass
class Alias:
    name: str
    type: object
    values: tuple = ()


@dataclass
class Registry:
    name: str
    keywords: dict = field(default_factory=dict)


def parse_type(text):
    """A type expression: a name, ("list", T), ("dict", K, V) or ("union", ...)."""
    if isinstance(text, list):
        return ("union", tuple(parse_type(item) for item in text))
    text = str(text).strip()
    match = re.fullmatch(r"(list|dict)\[(.*)\]", text)
    if not match:
        return text
    depth, parts, current = 0, [], ""
    for character in match.group(2):
        if character == "[":
            depth += 1
        elif character == "]":
            depth -= 1
        if character == "," and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += character
    parts.append(current)
    if match.group(1) == "list":
        return ("list", parse_type(parts[0]))
    return ("dict", parse_type(parts[0]), parse_type(parts[1]))


class Bundle:
    """One schema bundle: its root class and every named type it reaches."""

    def __init__(self, name, directory=SCHEMA_DIR):
        filename, self.root = BUNDLES[name]
        self.name = name
        self.classes = {}
        self.aliases = {}
        self.registries = {}
        raw_classes = {}
        seen = set()

        def load(path):
            if path in seen:
                return
            seen.add(path)
            document = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
            for include in document.get("includes", []) or []:
                load(path.parent / include)
            for key, value in document.items():
                if key in ("version", "includes"):
                    continue
                if isinstance(value, list):
                    raw_classes[key] = value
                elif isinstance(value, dict) and "registry" in value:
                    self.aliases[key] = Alias(key, ("registry", value["registry"]))
                elif isinstance(value, dict) and "type" in value:
                    self.aliases[key] = Alias(
                        key, parse_type(value["type"]), tuple(value.get("values") or ())
                    )
                elif isinstance(value, dict):
                    registry = self.registries.setdefault(key, Registry(key))
                    for keyword, payload in value.items():
                        registry.keywords[keyword] = self._payload(
                            f"{key}.{keyword}", payload, raw_classes
                        )

        load(directory / filename)
        for name_, entries in raw_classes.items():
            self.classes.setdefault(name_, Class(name_))
        for name_, entries in raw_classes.items():
            self.classes[name_].fields = self._fields(entries, raw_classes)

    def _payload(self, owner, payload, raw_classes):
        if isinstance(payload, list):
            raw_classes[owner] = payload
            return owner
        if isinstance(payload, dict) and "type" in payload:
            return parse_type(payload["type"])
        return "null"

    def _fields(self, entries, raw_classes, stack=()):
        fields = {}
        for entry in entries:
            for key, descriptor in entry.items():
                if key == "fields_from":
                    if descriptor in stack:
                        raise ValueError(f"cyclic fields_from {descriptor}")
                    fields.update(
                        self._fields(
                            raw_classes[descriptor], raw_classes, (*stack, descriptor)
                        )
                    )
                    continue
                descriptor = descriptor or {}
                fields[key] = Field(
                    key,
                    parse_type(descriptor.get("type", "str")),
                    bool(descriptor.get("required", False)),
                    descriptor.get("default"),
                    tuple(descriptor.get("values") or ()),
                )
        return fields

    # -- shapes ------------------------------------------------------------

    def expand(self, type_expr, seen=()):
        """Follow aliases to a concrete type expression."""
        if (
            isinstance(type_expr, str)
            and type_expr in self.aliases
            and type_expr not in seen
        ):
            return self.expand(self.aliases[type_expr].type, (*seen, type_expr))
        return type_expr

    def shape(self, type_expr, node):
        """The concrete shape `node` takes under `type_expr`, or None."""
        expanded = self.expand(type_expr)
        if isinstance(expanded, tuple) and expanded[0] == "union":
            best, score = None, None
            for option in expanded[1]:
                candidate = self.shape(option, node)
                if candidate is None:
                    continue
                fit = self.fit(candidate, node)
                if fit is not None and (score is None or fit > score):
                    best, score = candidate, fit
            return best
        if isinstance(expanded, tuple) and expanded[0] == "list":
            return expanded if isinstance(node, yaml.SequenceNode) else None
        if isinstance(expanded, tuple) and expanded[0] == "dict":
            return expanded if isinstance(node, yaml.MappingNode) else None
        if isinstance(expanded, tuple) and expanded[0] == "registry":
            registry = self.registries[expanded[1]]
            return (
                ("registry", registry.name)
                if isinstance(node, yaml.MappingNode)
                else None
            )
        if isinstance(expanded, str) and expanded in self.classes:
            return ("class", expanded) if isinstance(node, yaml.MappingNode) else None
        if isinstance(expanded, str) and expanded in self.registries:
            return (
                ("registry", expanded) if isinstance(node, yaml.MappingNode) else None
            )
        if isinstance(node, yaml.ScalarNode):
            return ("scalar", expanded if isinstance(expanded, str) else "str")
        return None

    def fit(self, shape, node):
        """How well a candidate shape fits a node; None when it does not."""
        kind = shape[0]
        if kind == "scalar":
            wanted = shape[1]
            actual = core_type(node)
            if wanted == "null":
                return 3 if actual == "null" else None
            if wanted == "bool":
                return 3 if actual == "bool" else None
            if wanted == "int":
                return 3 if actual == "int" else None
            if wanted == "float":
                return 2 if actual in ("int", "float") else None
            return 1 if actual == "str" else None
        if kind == "registry":
            if len(node.value) != 1:
                return None
            keyword = node.value[0][0].value
            registry = self.registries[shape[1]]
            if keyword not in registry.keywords:
                return None
            payload = node.value[0][1]
            payload_shape = self.shape(registry.keywords[keyword], payload)
            if payload_shape is None or self.fit(payload_shape, payload) is None:
                # The keyword names the registry, but its payload is not
                # admitted: still that registry's value (a fault to report),
                # unless a class whose fields fit is offered beside it.
                return 5
            return 100
        if kind == "class":
            fields = self.classes[shape[1]].fields
            known = sum(1 for key, _ in node.value if key.value in fields)
            unknown = len(node.value) - known
            if node.value and not known:
                return None
            score = 10 * known - 20 * unknown
            for key, value in node.value:
                descriptor = fields.get(key.value)
                if descriptor is None:
                    continue
                inner = self.shape(descriptor.type, value)
                if inner is None or (
                    inner[0] == "scalar"
                    and self.fit(inner, value) is None
                    and core_type(value) != "null"
                ):
                    score -= 15
            return score
        if kind in ("list", "dict"):
            return 0
        return None

    # -- walking -----------------------------------------------------------

    def walk(self, node, type_expr, path=(), holder=None):
        """Yield a Visit for every attributed node under `node`, in document order."""
        shape = self.shape(type_expr, node)
        if shape is None:
            return
        kind = shape[0]
        if kind == "class":
            fields = self.classes[shape[1]].fields
            for key, value in node.value:
                descriptor = fields.get(key.value)
                visit = Visit(
                    "field" if descriptor else "unknown",
                    shape[1],
                    key.value,
                    key,
                    value,
                    descriptor.type if descriptor else None,
                    node,
                    (*path, key.value),
                    holder,
                    descriptor,
                )
                yield visit
                if descriptor:
                    yield from self.walk(value, descriptor.type, visit.path, visit)
        elif kind == "registry":
            registry = self.registries[shape[1]]
            if len(node.value) != 1 or node.value[0][0].value not in registry.keywords:
                for key, value in node.value:
                    yield Visit(
                        "unknown",
                        registry.name,
                        key.value,
                        key,
                        value,
                        None,
                        node,
                        (*path, key.value),
                        holder,
                        None,
                    )
                return
            key, value = node.value[0]
            payload = registry.keywords[key.value]
            visit = Visit(
                "keyword",
                registry.name,
                key.value,
                key,
                value,
                payload,
                node,
                (*path, key.value),
                holder,
                None,
            )
            yield visit
            yield from self.walk(value, payload, visit.path, visit)
        elif kind == "list":
            for index, item in enumerate(node.value):
                visit = Visit(
                    "item",
                    None,
                    str(index),
                    None,
                    item,
                    shape[1],
                    node,
                    (*path, index),
                    holder,
                    None,
                )
                yield visit
                yield from self.walk(item, shape[1], visit.path, visit)
        elif kind == "dict":
            for key, value in node.value:
                visit = Visit(
                    "entry",
                    None,
                    key.value,
                    key,
                    value,
                    shape[2],
                    node,
                    (*path, key.value),
                    holder,
                    None,
                )
                yield visit
                yield from self.walk(value, shape[2], visit.path, visit)


@dataclass
class Visit:
    """One attributed node: a class field, a registry keyword, a list item or a map entry."""

    kind: str
    owner: object
    name: str
    key: object
    value: object
    type: object
    parent: object
    path: tuple
    holder: object
    field: object

    def location(self):
        """The rename table's `#owner.field` fragment for this node, or None."""
        if self.kind in ("field", "keyword") and self.owner:
            return f"{self.owner}.{self.name}"
        return None


_BUNDLES = {}


def bundle(name, directory=SCHEMA_DIR):
    key = (name, Path(directory))
    if key not in _BUNDLES:
        _BUNDLES[key] = Bundle(name, directory)
    return _BUNDLES[key]
