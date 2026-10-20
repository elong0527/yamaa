"""Read a schema bundle written in the schema notation: names, types and kinds.

The notation is defined by ``rules/reference/schema-language.md``. The 1.0
schema files under ``yaml/v1/`` add two keywords (``yaml/README.md``):
``kind``, which a named value type declares to give the fields it types their
kind, and ``since``, the language version that introduced a class field.
The Rust engine owns admission; this reader serves the repository generators
and their tests, and refuses only what they cannot interpret.
"""

import re
from dataclasses import dataclass, field
from pathlib import Path

import yaml
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.repository_schema import SchemaSourceError

BUILTINS = ("str", "int", "float", "bool", "null")
KINDS = ("expression", "name", "identifier", "value", "path", "structure")
DESCRIPTOR_KEYWORDS = frozenset(
    {"type", "description", "pattern", "min_length", "size", "values", "default"}
)
NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
# A language version, ``1.N`` with no leading zero (SCHEMA-2).
LANGUAGE_VERSION = re.compile(r"1\.(?:0|[1-9][0-9]*)")
TOKEN = re.compile(r"\s*([A-Za-z_][A-Za-z0-9_]*|\[|\]|,)")


@dataclass(frozen=True)
class Type:
    """One parsed type expression: a name, ``list[T]`` or ``dict[K, V]``."""

    name: str
    arguments: tuple = ()

    def __str__(self):
        if not self.arguments:
            return self.name
        return f"{self.name}[{', '.join(map(str, self.arguments))}]"


def parse_type(text):
    """Parse one type expression (SCHEMA-9)."""
    tokens = []
    position = 0
    text = str(text)
    while position < len(text.rstrip()):
        match = TOKEN.match(text, position)
        if match is None:
            raise SchemaSourceError(f"invalid type expression: {text!r}")
        tokens.append(match[1])
        position = match.end()

    def expression(index):
        if index >= len(tokens) or not NAME.fullmatch(tokens[index]):
            raise SchemaSourceError(f"invalid type expression: {text!r}")
        name = tokens[index]
        if name not in {"list", "dict"}:
            return Type(name), index + 1
        if index + 1 >= len(tokens) or tokens[index + 1] != "[":
            raise SchemaSourceError(f"invalid type expression: {text!r}")
        first, index = expression(index + 2)
        arguments = [first]
        if name == "dict":
            if index >= len(tokens) or tokens[index] != ",":
                raise SchemaSourceError(f"invalid type expression: {text!r}")
            second, index = expression(index + 1)
            arguments.append(second)
        if index >= len(tokens) or tokens[index] != "]":
            raise SchemaSourceError(f"invalid type expression: {text!r}")
        return Type(name, tuple(arguments)), index + 1

    parsed, end = expression(0)
    if end != len(tokens):
        raise SchemaSourceError(f"invalid type expression: {text!r}")
    return parsed


def type_text(descriptor):
    """The descriptor's written type: one expression, or a bracketed union."""
    written = descriptor["type"]
    if isinstance(written, list):
        return "[" + ", ".join(str(item) for item in written) + "]"
    return str(written)


def load_document(path):
    """Read one schema module under the YAML 1.2 core schema (SCHEMA-3, SCHEMA-4)."""
    try:
        document = yaml.load(path.read_text(encoding="ascii"), Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as exc:
        raise SchemaSourceError(f"{path.name}: invalid schema source: {exc}") from exc
    if not isinstance(document, dict):
        raise SchemaSourceError(f"{path.name}: expected schema mapping")
    return document


@dataclass(frozen=True)
class Field:
    """A class field after ``fields_from`` expansion, with the class declaring it."""

    name: str
    descriptor: dict = field(compare=False)
    origin: str


class Bundle:
    """The declarations of schema modules, read together.

    ``entry`` loads that module and its transitive includes, as one bundle;
    without it, every ``schema*.yaml`` module of the directory is read into
    one namespace, which needs declaration names unique across the directory.
    """

    def __init__(self, directory, entry=None):
        self.directory = Path(directory)
        self.modules = {}
        if entry is None:
            for path in sorted(self.directory.glob("schema*.yaml")):
                self.modules[path.name] = load_document(path)
        else:
            self._include(entry)
        self.classes = {}
        self.aliases = {}
        self.registries = {}
        self.module_of = {}
        for module, document in self.modules.items():
            for name, value in document.items():
                if name in {"version", "includes"}:
                    continue
                if isinstance(value, list):
                    self._declare(self.classes, name, value, module)
                elif isinstance(value, dict) and (
                    "type" in value or "registry" in value
                ):
                    self._declare(self.aliases, name, value, module)
                    self._check_alias(name, value)
                elif isinstance(value, dict):
                    entries = self.registries.setdefault(name, {})
                    for keyword, payload in value.items():
                        if keyword in entries:
                            raise SchemaSourceError(
                                f"{module}: duplicate registry entry {name}.{keyword}"
                            )
                        entries[keyword] = payload
                else:
                    raise SchemaSourceError(f"{module}: unknown declaration {name}")
        self._fields = {}
        self._kinds = {}
        self._holds = {}

    def _include(self, name):
        if name in self.modules:
            return
        path = self.directory / name
        if not path.is_file():
            raise SchemaSourceError(f"missing schema module {name}")
        document = load_document(path)
        self.modules[name] = document
        for included in document.get("includes", []):
            self._include(included)

    def _declare(self, table, name, value, module):
        if name in self.classes or name in self.aliases:
            raise SchemaSourceError(f"{module}: duplicate declaration {name}")
        table[name] = value
        self.module_of[name] = module

    def _check_alias(self, name, alias):
        if "registry" in alias:
            if set(alias) != {"registry"}:
                raise SchemaSourceError(
                    f"{name}: a registry type declares only registry"
                )
            return
        unknown = set(alias) - DESCRIPTOR_KEYWORDS - {"kind"}
        if unknown:
            raise SchemaSourceError(f"{name}: unknown keyword {sorted(unknown)[0]}")
        if "kind" in alias and alias["kind"] not in KINDS:
            raise SchemaSourceError(f"{name}: kind must be one of {', '.join(KINDS)}")

    # -- names and fields -------------------------------------------------

    def members(self, descriptor):
        """The parsed members of a descriptor's type."""
        written = descriptor["type"]
        items = written if isinstance(written, list) else [written]
        return [parse_type(item) for item in items]

    def fields(self, class_name, active=()):
        """A class's fields in schema order, ``fields_from`` expanded (SCHEMA-6)."""
        if class_name in self._fields:
            return self._fields[class_name]
        if class_name in active:
            raise SchemaSourceError(f"fields_from cycle at {class_name}")
        if class_name not in self.classes:
            raise SchemaSourceError(f"unknown class {class_name}")
        result = []
        for entry in self.classes[class_name]:
            if not isinstance(entry, dict) or len(entry) != 1:
                raise SchemaSourceError(
                    f"{class_name}: a field entry is a one-entry mapping"
                )
            ((name, descriptor),) = entry.items()
            if name == "fields_from":
                result.extend(self.fields(descriptor, (*active, class_name)))
            else:
                self._check_field(class_name, name, descriptor)
                result.append(Field(name, descriptor, class_name))
        names = [item.name for item in result]
        if len(names) != len(set(names)):
            raise SchemaSourceError(f"{class_name}: duplicate field after fields_from")
        self._fields[class_name] = result
        return result

    def _check_field(self, owner, name, descriptor):
        if not isinstance(descriptor, dict) or "type" not in descriptor:
            raise SchemaSourceError(f"{owner}.{name}: a field has a typed descriptor")
        unknown = set(descriptor) - DESCRIPTOR_KEYWORDS - {"required", "since"}
        if unknown:
            raise SchemaSourceError(
                f"{owner}.{name}: unknown keyword {sorted(unknown)[0]}"
                + (" (a named value type declares kind)" if "kind" in unknown else "")
            )
        since = descriptor.get("since")
        if since is not None and not (
            isinstance(since, str) and LANGUAGE_VERSION.fullmatch(since)
        ):
            raise SchemaSourceError(
                f"{owner}.{name}: since must be a quoted language version 1.N"
            )

    def payload_fields(self, registry, keyword):
        """The fields of a registry entry whose payload is a class."""
        owner = f"{registry}.{keyword}"
        if owner not in self._fields:
            payload = self.registries[registry][keyword]
            fields = []
            for entry in payload:
                ((name, descriptor),) = entry.items()
                if name == "fields_from":
                    fields.extend(self.fields(descriptor))
                else:
                    self._check_field(owner, name, descriptor)
                    fields.append(Field(name, descriptor, owner))
            self._fields[owner] = fields
        return self._fields[owner]

    def owners(self):
        """Every class and class-shaped registry entry, by owner name, with fields."""
        result = {name: self.fields(name) for name in self.classes}
        for registry, entries in self.registries.items():
            for keyword, payload in entries.items():
                if isinstance(payload, list):
                    result[f"{registry}.{keyword}"] = self.payload_fields(
                        registry, keyword
                    )
        return result

    def reachable(self, root):
        """Owner names reachable as types from a root class, in discovery order."""
        seen = {}
        pending = [root]
        names = set()

        def visit(descriptor):
            for member in self.members(descriptor):
                stack = [member]
                while stack:
                    item = stack.pop()
                    stack.extend(item.arguments)
                    if item.name not in BUILTINS and item.name not in {"list", "dict"}:
                        pending.append(item.name)

        while pending:
            name = pending.pop(0)
            if name in names:
                continue
            names.add(name)
            if name in self.classes:
                seen[name] = None
                for item in self.fields(name):
                    visit(item.descriptor)
            elif name in self.aliases:
                alias = self.aliases[name]
                if "registry" in alias:
                    for keyword, payload in self.registries[alias["registry"]].items():
                        if isinstance(payload, list):
                            seen[f"{alias['registry']}.{keyword}"] = None
                            for item in self.payload_fields(alias["registry"], keyword):
                                visit(item.descriptor)
                        else:
                            visit(payload)
                else:
                    visit(alias)
            else:
                raise SchemaSourceError(f"unknown type {name}")
        return list(seen)

    # -- kinds and values -------------------------------------------------

    def kind(self, item, active=()):
        """The kind of one parsed type."""
        if item.name in BUILTINS:
            return "value"
        if item.name == "list":
            return self.kind(item.arguments[0], active)
        if item.name == "dict":
            return self.kind(item.arguments[1], active)
        if item.name in self.classes:
            return "structure"
        if item.name not in self.aliases:
            raise SchemaSourceError(f"unknown type {item.name}")
        if item.name in self._kinds:
            return self._kinds[item.name]
        if item.name in active:
            raise SchemaSourceError(f"{item.name}: a recursive type declares its kind")
        alias = self.aliases[item.name]
        if "registry" in alias:
            kind = "structure"
        else:
            kind = self.descriptor_kind(alias, item.name, (*active, item.name))
        self._kinds[item.name] = kind
        return kind

    def descriptor_kind(self, descriptor, label, active=()):
        """A descriptor's kind: the one it declares, else the one its members share."""
        if "kind" in descriptor:
            return descriptor["kind"]
        kinds = {self.kind(member, active) for member in self.members(descriptor)}
        if len(kinds) != 1:
            raise SchemaSourceError(
                f"{label}: a union of {', '.join(sorted(kinds))} members is a named "
                "type that declares its kind"
            )
        return kinds.pop()

    def holds_class(self, name, active=()):
        """Whether a named type reaches a class, so requiredness applies inside it."""
        if name in self.classes:
            return True
        if name in self._holds:
            return self._holds[name]
        if name in active or name not in self.aliases:
            return False
        alias = self.aliases[name]
        if "registry" in alias:
            payloads = self.registries[alias["registry"]].values()
            holds = any(
                isinstance(payload, list)
                or self._descriptor_holds(payload, (*active, name))
                for payload in payloads
            )
        else:
            holds = self._descriptor_holds(alias, (*active, name))
        self._holds[name] = holds
        return holds

    def _descriptor_holds(self, descriptor, active):
        stack = list(self.members(descriptor))
        while stack:
            item = stack.pop()
            stack.extend(item.arguments)
            if item.name not in BUILTINS and item.name not in {"list", "dict"}:
                if self.holds_class(item.name, active):
                    return True
        return False

    def values(self, descriptor):
        """The permitted values a descriptor's type spells, through named types."""
        if "values" in descriptor:
            return tuple(descriptor["values"])
        found = []
        for member in self.members(descriptor):
            found.extend(self._type_values(member))
        return tuple(found) or None

    def _type_values(self, item):
        if item.name == "list":
            return self._type_values(item.arguments[0])
        if item.name == "dict":
            return self._type_values(item.arguments[1])
        alias = self.aliases.get(item.name)
        if alias is None or "registry" in alias:
            return ()
        return self.values(alias) or ()
