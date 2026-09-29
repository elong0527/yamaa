"""Specification composition (specification/composition).

An entry file that names `parents` resolves into one resolved specification
before any other rule reads it (REQ-0651). Each layer is read as a
schema-shaped fragment: shorthand is expanded and every path it writes is
rebased to the entry file. The layers then merge in contribution order, and
the result is materialized, pruned to what can affect the artifact, ordered
by column dependency, and laid out in schema order (REQ-0614 onward).

Composition and the canonical form both follow each field's declared kind
(REQ-0626, REQ-0630, REQ-0646), so this module reads the kinds from the
schema bundle under `yaml/` rather than restating them. Errors a resolved
specification can still carry -- an unknown reference, a dependency cycle,
an incomplete object -- are left to the engine's validation, which applies
to the resolved specification exactly as to a single file.
"""

import copy
import functools
import os
import re

import yaml

from . import agg as _agg
from . import numeric as _numeric
from . import pred as _pred
from . import validate as _validate
from .errors import YamaaError

_URI_SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
_DRIVE_ROOT = re.compile(r"^[A-Za-z]:[\\/]")
_TEMPLATE_FIELD = re.compile(r"{{|}}|{([A-Za-z_][A-Za-z0-9_.]*)}")

# REQ-0628: the keyed root collections, their identity, and member class.
_KEYED = {
    "input": (None, "dataset_class"),
    "intermediates": ("id", "intermediate_class"),
    "columns": ("name", "column_class"),
    "rows": ("id", "row_class"),
}

# REQ-0630: a `key` states one match whatever its form, so it replaces whole
# rather than composing its column-to-value mapping key by key.
_REPLACED_WHOLE = {"match_key"}

# Positions where an `identifier` names a dataset or a column (REQ-0639). A
# list-form lookup or aggregate `key` names the current row's columns.
_DATASET_FIELDS = {
    ("root_class", "base"),
    ("row_class", "dataset"),
    ("intermediate_class", "dataset"),
}
_COLUMN_FIELDS = {
    ("column_class", "name"),
    ("root_class", "keys"),
    ("output_class", "columns"),
    ("intermediate_class", "key"),
    ("aggregate_class", "key"),
}

_KEEP = object()  # a visitor's answer: descend into the value unchanged


def _fail(where, condition, requirement, context):
    raise YamaaError(
        phase="validation",
        condition=condition,
        requirement=requirement,
        spec_paths=[where],
        context=context,
    )


def _members(type_value):
    values = type_value if isinstance(type_value, list) else [type_value]
    return [str(v).strip() for v in values]


def _list_inner(name):
    if name.startswith("list[") and name.endswith("]"):
        return name[5:-1].strip()
    return None


def _dict_value(name):
    """The value type of `dict[K, V]`, or None for any other type."""
    if not (name.startswith("dict[") and name.endswith("]")):
        return None
    depth = 0
    for i, ch in enumerate(name[5:-1]):
        if ch == "[":
            depth += 1
        elif ch == "]":
            depth -= 1
        elif ch == "," and depth == 0:
            return name[5:-1][i + 1 :].strip()
    return None


class Bundle:
    """The schema bundle's classes, aliases, and registries (REQ-0250 on).

    A class maps each field to its descriptor in schema order, `fields_from`
    already expanded; a registry maps each keyword to its payload, either
    ("class", fields) or ("value", descriptor).
    """

    def __init__(self, root):
        raw, self.aliases, registries = {}, {}, {}
        self.version = None
        pending, seen = ["schema.yaml"], set()
        while pending:
            name = pending.pop()
            if name in seen:
                continue
            seen.add(name)
            with open(os.path.join(root, name), "r", encoding="utf-8") as f:
                doc = yaml.safe_load(f)
            self.version = self.version or doc.get("version")
            pending.extend(doc.get("includes") or [])
            for key, d in doc.items():
                if key in ("version", "includes"):
                    continue
                if isinstance(d, list):
                    raw[key] = d
                elif isinstance(d.get("registry"), str) or isinstance(
                    d.get("type"), (str, list)
                ):
                    self.aliases[key] = d
                else:
                    registries.setdefault(key, {}).update(d)
        self.classes = {}

        def expand(entries):
            fields = {}
            for entry in entries:
                if "fields_from" in entry:
                    fields.update(expand(raw[entry["fields_from"]]))
                else:
                    fields.update(entry)
            return fields

        for key, entries in raw.items():
            self.classes[key] = expand(entries)
        self.registries = {
            key: {
                kw: ("class", expand(p)) if isinstance(p, list) else ("value", p)
                for kw, p in entries.items()
            }
            for key, entries in registries.items()
        }

    # ------------------------------------------------------------ matching

    def matches(self, v, type_value, fragment):
        """True when v has the shape of one union member (REQ-0259, REQ-0261).

        Only the shape decides; constraints such as a pattern are the
        engine's to report. `fragment` defers requiredness at every depth.
        """
        return self.member(v, type_value, fragment) is not None

    def member(self, v, type_value, fragment):
        for m in _members(type_value):
            if self._matches(v, m, fragment):
                return m
        return None

    def _matches(self, v, t, fragment):
        if t == "str":
            return isinstance(v, str)
        if t == "int":
            return type(v) is int
        if t == "float":
            return type(v) in (int, float)
        if t == "bool":
            return type(v) is bool
        if t == "null":
            return v is None
        if t == "list":
            return isinstance(v, list)
        if t == "dict":
            return isinstance(v, dict)
        inner = _list_inner(t)
        if inner is not None:
            return isinstance(v, list) and all(
                self.matches(x, inner, fragment) for x in v
            )
        value_type = _dict_value(t)
        if value_type is not None:
            return isinstance(v, dict) and all(
                self.matches(x, value_type, fragment) for x in v.values()
            )
        if t in self.classes:
            return self._matches_class(v, self.classes[t], fragment)
        alias = self.aliases.get(t)
        if alias is None:
            return False
        if "registry" in alias:
            payload = self._payload(v, alias["registry"])
            if payload is None:
                return False
            kind, shape = payload[1]
            if kind == "class":
                return self._matches_class(payload[0], shape, fragment)
            return self.matches(payload[0], shape["type"], fragment)
        return self.matches(v, alias["type"], fragment)

    def _matches_class(self, v, fields, fragment):
        return (
            isinstance(v, dict)
            and all(k in fields for k in v)
            and (
                fragment or all(k in v for k, d in fields.items() if d.get("required"))
            )
            and all(self.matches(x, fields[k]["type"], fragment) for k, x in v.items())
        )

    def _payload(self, v, registry):
        """(payload, declaration) of a one-keyword registry value, or None."""
        if not isinstance(v, dict) or len(v) != 1:
            return None
        kw, payload = next(iter(v.items()))
        declaration = self.registries.get(registry, {}).get(kw)
        return None if declaration is None else (payload, declaration)

    # ------------------------------------------------------- normalization

    def normalize(self, v, type_value, fragment=False, rebase=None):
        """The canonical form of v (REQ-0264 to REQ-0268).

        `T | list[T]` expands a bare T to a list; `V | class` expands a bare
        V into the class's one required field, applying the class defaults.
        A class also gains its declared defaults unless `fragment`, which a
        `columns` patch reads under so that a default never replaces what an
        earlier layer wrote (REQ-0630). `rebase` respells each written path.
        A value no member describes is kept for the engine to report.
        """
        members = _members(type_value)
        if len(members) == 2:
            for m in members:
                inner = _list_inner(m)
                if inner is None or inner not in members:
                    continue
                if self._matches(v, m, fragment):
                    return self._normalize_member(v, m, fragment, rebase)
                if self._matches(v, inner, fragment):
                    return [self.normalize(v, inner, fragment, rebase)]
            for c in members:
                fields = self.classes.get(c)
                if fields is None:
                    continue
                required = [(n, d) for n, d in fields.items() if d.get("required")]
                other = next(m for m in members if m != c)
                if (
                    len(required) != 1
                    or other in self.classes
                    or _members(required[0][1]["type"]) != [other]
                    or not self._matches(v, other, fragment)
                ):
                    continue
                field, descriptor = required[0]
                out = {field: self.normalize(v, descriptor["type"], fragment, rebase)}
                if not fragment:
                    self._defaults(out, fields)
                return out
        for m in members:
            if self._matches(v, m, fragment):
                return self._normalize_member(v, m, fragment, rebase)
        return copy.deepcopy(v)

    def _normalize_member(self, v, m, fragment, rebase):
        if m in ("path", "project_path") and isinstance(v, str) and rebase:
            return rebase(v)
        inner = _list_inner(m)
        if inner is not None:
            return [self.normalize(x, inner, fragment, rebase) for x in v]
        value_type = _dict_value(m)
        if value_type is not None:
            return {
                k: self.normalize(x, value_type, fragment, rebase) for k, x in v.items()
            }
        if m in self.classes:
            return self.normalize_class(v, self.classes[m], fragment, rebase)
        alias = self.aliases.get(m)
        if alias is None:
            return copy.deepcopy(v)
        if m in ("derivation", "case_result"):
            # REQ-0319: a bare string is the source shorthand; the `str`
            # member exists for validation, never for the canonical form.
            if isinstance(v, str):
                v = {"source": v}
            rest = [t for t in _members(alias["type"]) if t != "str"]
            return self.normalize(v, rest, fragment, rebase)
        if "registry" in alias:
            payload, (kind, shape) = self._payload(v, alias["registry"])
            kw = next(iter(v))
            if kind == "class":
                return {kw: self.normalize_class(payload, shape, fragment, rebase)}
            return {kw: self.normalize(payload, shape["type"], fragment, rebase)}
        return self.normalize(v, alias["type"], fragment, rebase)

    def normalize_class(self, v, fields, fragment=False, rebase=None):
        if not isinstance(v, dict):
            return copy.deepcopy(v)
        out = {}
        for n, d in fields.items():
            if n in v:
                out[n] = self.normalize(v[n], d["type"], fragment, rebase)
        if not fragment:
            self._defaults(out, fields)
        # An undeclared field is the engine's to report, not composition's.
        out.update({n: copy.deepcopy(x) for n, x in v.items() if n not in fields})
        return self.ordered(out, fields)

    def _defaults(self, out, fields):
        for n, d in fields.items():
            if n not in out and "default" in d:
                out[n] = self.normalize(copy.deepcopy(d["default"]), d["type"])

    @staticmethod
    def ordered(value, fields):
        """REQ-0649: declared fields in schema order, then any other field."""
        out = {n: value[n] for n in fields if n in value}
        out.update({n: x for n, x in value.items() if n not in fields})
        return out

    # ----------------------------------------------------------- traversal

    def visit(self, v, type_value, fn, scope=None):
        """Rebuild v, letting fn(type_name, value, scope) replace any node.

        fn returns _KEEP to descend. `scope` is the (class or keyword, field)
        that holds the value, carried through aliases and lists.
        """
        m = self.member(v, type_value, True) or self._outer(v, type_value)
        if m is None:
            return v
        return self._visit_member(v, m, fn, scope)

    def _outer(self, v, type_value):
        """The first member whose container v fits, so that one undeclared
        field does not hide the references around it."""
        for m in _members(type_value):
            if m in ("str", "int", "float", "bool", "null", "list", "dict"):
                if self._matches(v, m, True):
                    return m
            elif _list_inner(m) is not None:
                if isinstance(v, list):
                    return m
            elif _dict_value(m) is not None or m in self.classes:
                if isinstance(v, dict):
                    return m
            elif m in self.aliases:
                alias = self.aliases[m]
                if "registry" in alias:
                    if self._payload(v, alias["registry"]) is not None:
                        return m
                elif self._outer(v, alias["type"]) is not None:
                    return m
        return None

    def _visit_member(self, v, m, fn, scope):
        answer = fn(m, v, scope)
        if answer is not _KEEP:
            return answer
        inner = _list_inner(m)
        if inner is not None:
            return [self.visit(x, inner, fn, scope) for x in v]
        value_type = _dict_value(m)
        if value_type is not None:
            return {k: self.visit(x, value_type, fn, scope) for k, x in v.items()}
        if m in self.classes:
            return self._visit_class(v, self.classes[m], m, fn)
        alias = self.aliases.get(m)
        if alias is None:
            return v
        if "registry" in alias:
            payload, (kind, shape) = self._payload(v, alias["registry"])
            kw = next(iter(v))
            if kind == "class":
                if not isinstance(payload, dict):
                    return v
                return {kw: self._visit_class(payload, shape, kw, fn)}
            return {kw: self.visit(payload, shape["type"], fn, scope)}
        return self.visit(v, alias["type"], fn, scope)

    def _visit_class(self, v, fields, owner, fn):
        return {
            n: self.visit(x, fields[n]["type"], fn, (owner, n)) if n in fields else x
            for n, x in v.items()
        }

    def references(self, v, type_value, scope=None):
        """The (kind, name) references a value makes (REQ-0639).

        A variable, or an identifier where it names a dataset or a column,
        and every name a predicate, formula, aggregate, or template reads.
        """
        found = set()

        def fn(m, x, where):
            if not isinstance(x, str):
                return _KEEP
            if m == "variable":
                found.add(("variable", x))
            elif m == "identifier" and where in _DATASET_FIELDS:
                found.add(("dataset", x))
            elif m == "identifier" and where in _COLUMN_FIELDS:
                found.add(("variable", x))
            elif m == "odm_item":
                found.add(("dataset", x.split(".", 1)[0]))
            elif m in _LANGUAGES:
                found.update(("variable", n) for n in _LANGUAGES[m](x))
            else:
                return _KEEP
            return x

        self.visit(v, type_value, fn, scope)
        return found


def _parsed_names(parse):
    def names(text):
        try:
            return parse(text, "<composition>")[1]
        except YamaaError:
            return []  # a malformed expression is the engine's to report

    return names


def _template_names(text):
    return [m.group(1) for m in _TEMPLATE_FIELD.finditer(text) if m.group(1)]


_LANGUAGES = {
    "predicate": _parsed_names(_pred.parse),
    "numeric_expression": _parsed_names(_numeric.parse),
    "aggregate_expression": _parsed_names(_agg.parse),
    "string_template": _template_names,
}


def _bundle_root(entry_dir):
    """The `yaml/` bundle beside this package's checkout, else above the entry."""
    for start in (os.path.dirname(os.path.abspath(__file__)), entry_dir):
        d = start
        while True:
            if os.path.isfile(os.path.join(d, "yaml", "schema.yaml")):
                return os.path.join(d, "yaml")
            parent = os.path.dirname(d)
            if parent == d:
                break
            d = parent
    raise RuntimeError(
        "spec composition needs the schema bundle (yaml/schema.yaml), "
        f"found neither above {os.path.dirname(os.path.abspath(__file__))} "
        f"nor above {entry_dir}"
    )


@functools.cache
def load_bundle(root):
    return Bundle(root)


# ------------------------------------------------------------------ layers


def _nonlocal(parent):
    """REQ-0618: a URL or URI, including `file:`, is not a parent path."""
    if not isinstance(parent, str) or not parent:
        return True
    return not _DRIVE_ROOT.match(parent) and bool(_URI_SCHEME.match(parent))


def _rooted(path):
    return path.startswith("/") or bool(re.match(r"^[A-Za-z]:/", path))


class _Layer:
    """One normalized layer: its canonical file, its fields, and the
    relative project paths it wrote, as it spelled them (REQ-0616)."""

    def __init__(self, path, fields, written):
        self.path = path
        self.fields = fields
        self.written = written


def _read_layer(bundle, path, doc, entry_dir):
    if not isinstance(doc, dict) or not doc:
        _fail(
            "parents",
            "invalid_field_type",
            "REQ-0622",
            {"expected": "root_class", "path": path},
        )
    layer_dir = os.path.dirname(path)
    written = {}

    def rebaser(logical):
        def rebase(value):
            # REQ-0635/REQ-0636: a relative path is the layer's; restate it
            # from the entry without changing the file it denotes.
            if _rooted(value) or _URI_SCHEME.match(value) or layer_dir == entry_dir:
                return value
            if logical is not None:
                written[logical] = (layer_dir, value)
            target = os.path.normpath(os.path.join(layer_dir, value))
            return os.path.relpath(target, entry_dir).replace(os.sep, "/")

        return rebase

    root = bundle.classes["root_class"]
    fields = {}
    for name, value in doc.items():
        descriptor = root.get(name)
        if name == "parents":
            fields[name] = bundle.normalize(value, descriptor["type"])
        elif descriptor is None or name == "schema_version":
            fields[name] = copy.deepcopy(value)
        elif value is None:
            # REQ-0632: a null clears an inherited optional field.
            if descriptor.get("required"):
                _fail(name, "invalid_clear", "REQ-0660", {"field": name})
            fields[name] = None
        elif name == "input" and isinstance(value, dict):
            members = {}
            for dataset, member in value.items():
                where = f"input.{dataset}"
                if isinstance(member, str):
                    member = {"path": member}
                members[dataset] = _read_member(
                    bundle,
                    member,
                    "dataset_class",
                    None,
                    where,
                    False,
                    lambda f, w=where: rebaser(f"{w}.{f}"),
                )
            fields[name] = members
        elif name in _KEYED and isinstance(value, list):
            identity, class_name = _KEYED[name]
            members, seen = [], set()
            for i, member in enumerate(value):
                key = member.get(identity) if isinstance(member, dict) else None
                if key is None and isinstance(member, dict) and identity in member:
                    # REQ-0632: an identity field cannot be cleared.
                    _fail(
                        f"{name}[{i}].{identity}",
                        "invalid_clear",
                        "REQ-0660",
                        {"field": identity},
                    )
                if not isinstance(key, str):
                    _fail(
                        f"{name}[{i}].{identity}",
                        "missing_required_field",
                        "REQ-0624",
                        {"field": identity, "class": class_name},
                    )
                if key in seen:
                    _fail(
                        f"{name}.{key}.{identity}",
                        "duplicate_identifier",
                        "REQ-0659",
                        {"identifier": key},
                    )
                seen.add(key)
                members.append(
                    _read_member(
                        bundle,
                        member,
                        class_name,
                        identity,
                        f"{name}.{key}",
                        name == "columns",
                        lambda f: rebaser(None),
                    )
                )
            fields[name] = members
        else:
            fields[name] = bundle.normalize(
                value, descriptor["type"], rebase=rebaser(None)
            )
    return _Layer(path, fields, written)


def _read_member(bundle, member, class_name, identity, where, fragment, rebaser):
    """REQ-0625: a keyed member's fields, each complete at its boundary --
    except a `columns` field, which is a patch of what it composes onto."""
    if not isinstance(member, dict):
        return copy.deepcopy(member)
    fields = bundle.classes[class_name]
    out = {}
    for name, value in member.items():
        descriptor = fields.get(name)
        if descriptor is None:
            out[name] = copy.deepcopy(value)
        elif value is None:
            if descriptor.get("required") or name == identity:
                _fail(f"{where}.{name}", "invalid_clear", "REQ-0660", {"field": name})
            out[name] = None
        else:
            out[name] = bundle.normalize(
                value, descriptor["type"], fragment, rebaser(name)
            )
    return out


def _linearize(bundle, entry_path, entry_doc):
    """REQ-0620: parents left to right, depth first, then the layer; each
    layer contributes once, at its first visit."""
    entry_dir = os.path.dirname(entry_path)
    layers, active, completed = [], [], set()

    def visit(path, doc):
        if path in active:
            _fail(
                "parents",
                "inheritance_cycle",
                "REQ-0655",
                {"reason": "parent_chain_returns_to_entry"},
            )
        if path in completed:
            return
        if doc is None:
            with open(path, "r", encoding="utf-8") as f:
                doc = yaml.safe_load(f)
        layer = _read_layer(bundle, path, doc, entry_dir)
        version = layer.fields.get("schema_version")
        if version != bundle.version:
            _fail(
                "parents" if layers or active else "schema_version",
                "schema_version_mismatch",
                "REQ-0656",
                {"entry_version": bundle.version, "parent_version": version},
            )
        active.append(path)
        parents = layer.fields.get("parents") or []
        for parent in parents if isinstance(parents, list) else [parents]:
            if _nonlocal(parent):
                _fail(
                    "parents",
                    "invalid_parent_path",
                    "REQ-0653",
                    {
                        "reason": "remote_reference"
                        if isinstance(parent, str) and parent
                        else "empty_path"
                    },
                )
            full = os.path.join(os.path.dirname(path), parent)
            if not os.path.isfile(full):
                _fail("parents", "parent_not_found", "REQ-0654", {"path": parent})
            visit(os.path.realpath(full), None)
        active.pop()
        completed.add(path)
        layers.append(layer)

    visit(entry_path, entry_doc)
    return layers


# ------------------------------------------------------------------- merge


def _compose(bundle, acc, inc, type_value):
    """REQ-0630: compose a written value onto what it inherits, by kind.

    A class composes field by field, a mapping key by key, and a registry
    value only when both name one keyword. Every other kind, every list,
    and a value of another kind replaces. A null here is a value, never a
    clearing marker (REQ-0633).
    """
    m = bundle.member(inc, type_value, True)
    if (
        m is None
        or m in _REPLACED_WHOLE
        or not isinstance(acc, dict)
        or not isinstance(inc, dict)
        or m != bundle.member(acc, type_value, True)
    ):
        return copy.deepcopy(inc)
    if m in bundle.classes:
        return _compose_class(bundle, acc, inc, bundle.classes[m])
    value_type = _dict_value(m)
    if value_type is not None:
        out = dict(acc)
        for k, x in inc.items():
            out[k] = (
                _compose(bundle, out[k], x, value_type)
                if k in out
                else copy.deepcopy(x)
            )
        return out
    alias = bundle.aliases.get(m)
    if alias is None:
        return copy.deepcopy(inc)
    if "registry" not in alias:
        return _compose(bundle, acc, inc, alias["type"])
    kw, payload = next(iter(inc.items()))
    inherited_kw, inherited = next(iter(acc.items()))
    if kw != inherited_kw:
        return copy.deepcopy(inc)
    kind, shape = bundle.registries[alias["registry"]][kw]
    if kind == "value":
        return {kw: _compose(bundle, inherited, payload, shape["type"])}
    if not isinstance(inherited, dict) or not isinstance(payload, dict):
        return copy.deepcopy(inc)
    return {kw: _compose_class(bundle, inherited, payload, shape)}


def _compose_class(bundle, acc, inc, fields):
    out = dict(acc)
    for n, x in inc.items():
        if n in fields and n in out:
            out[n] = _compose(bundle, out[n], x, fields[n]["type"])
        else:
            out[n] = copy.deepcopy(x)
    return out


def _merge_member(bundle, acc, inc, class_name, where, compose):
    """REQ-0630: a matching member merges its immediate fields; a
    `columns` member composes each of them by its declared kind."""
    fields = bundle.classes[class_name]
    for n, x in inc.items():
        if x is None:
            if fields.get(n, {}).get("required") or n not in acc:
                _fail(f"{where}.{n}", "invalid_clear", "REQ-0660", {"field": n})
            del acc[n]
        elif compose and n in acc and n in fields:
            acc[n] = _compose(bundle, acc[n], x, fields[n]["type"])
        else:
            acc[n] = copy.deepcopy(x)


def _new_member(member, where):
    """REQ-0632: a null clears an inherited value; a first contribution has
    none to clear."""
    for n, x in member.items() if isinstance(member, dict) else ():
        if x is None:
            _fail(f"{where}.{n}", "invalid_clear", "REQ-0660", {"field": n})
    return copy.deepcopy(member)


def _merge(bundle, layers):
    """REQ-0627 to REQ-0632 and REQ-1254, in contribution order."""
    resolved, written = {}, {}
    for layer in layers:
        for name, value in layer.fields.items():
            if name == "parents":
                continue
            if value is None:
                if name not in resolved:
                    _fail(name, "invalid_clear", "REQ-0660", {"field": name})
                del resolved[name]
                if name == "input":
                    written.clear()
                continue
            target = resolved.get(name)
            if name == "windows" and isinstance(value, dict):
                # REQ-1254: each definition replaces the whole named window.
                target = target if isinstance(target, dict) else {}
                resolved[name] = {**target, **copy.deepcopy(value)}
            elif name == "input" and isinstance(value, dict):
                target = (
                    resolved.setdefault(name, {}) if isinstance(target, dict) else {}
                )
                resolved[name] = target
                for dataset, member in value.items():
                    where = f"input.{dataset}"
                    if dataset in target and isinstance(target[dataset], dict):
                        _merge_member(
                            bundle,
                            target[dataset],
                            member,
                            "dataset_class",
                            where,
                            False,
                        )
                    else:
                        target[dataset] = _new_member(member, where)
                    for field in member if isinstance(member, dict) else ():
                        logical = f"{where}.{field}"
                        written.pop(logical, None)
                        if logical in layer.written:
                            written[logical] = layer.written[logical]
            elif name in _KEYED and isinstance(value, list):
                identity, class_name = _KEYED[name]
                target = target if isinstance(target, list) else []
                resolved[name] = target
                positions = {m.get(identity): i for i, m in enumerate(target)}
                for member in value:
                    key = member[identity]
                    where = f"{name}.{key}"
                    if key in positions:
                        _merge_member(
                            bundle,
                            target[positions[key]],
                            member,
                            class_name,
                            where,
                            name == "columns",
                        )
                    else:
                        positions[key] = len(target)
                        target.append(_new_member(member, where))
            else:
                resolved[name] = copy.deepcopy(value)
    return resolved, written


def _materialize(bundle, resolved):
    """A composed column is complete only now: apply its defaults once, so a
    layer that never names a field cannot replace it with a default."""
    fields = bundle.classes["column_class"]
    for column in resolved.get("columns") or []:
        if isinstance(column, dict):
            for n, x in column.items():
                if n in fields and x is not None:
                    column[n] = bundle.normalize(x, fields[n]["type"])


def _expand_windows(bundle, resolved):
    """REQ-1253/REQ-1254: every reference, inherited ones included, takes a
    copy of the final composed definition. An unknown name stays in place
    for the engine to report."""
    windows = resolved.get("windows")
    if not isinstance(windows, dict):
        return resolved
    for name, w in windows.items():
        _validate.check_named_window_definition(name, w)

    def fn(m, v, scope):
        if m == "window_selection" and isinstance(v, str) and v in windows:
            return copy.deepcopy(windows[v])
        return _KEEP

    return _visit_document(
        bundle, {k: v for k, v in resolved.items() if k != "windows"}, fn
    )


def _visit_document(bundle, doc, fn):
    """Visit a resolved document field by field and member by member."""
    root = bundle.classes["root_class"]
    out = {}
    for name, value in doc.items():
        if name in _KEYED and isinstance(value, (dict, list)):
            fields = bundle.classes[_KEYED[name][1]]
            members = value.items() if isinstance(value, dict) else enumerate(value)
            visited = {
                k: bundle._visit_class(m, fields, _KEYED[name][1], fn)
                if isinstance(m, dict)
                else m
                for k, m in members
            }
            out[name] = visited if isinstance(value, dict) else list(visited.values())
        elif name in root:
            out[name] = bundle.visit(
                value, root[name]["type"], fn, ("root_class", name)
            )
        else:
            out[name] = value
    return out


# ------------------------------------------------------------------- prune


def _prune(bundle, doc):
    """REQ-0638 to REQ-0641: keep only what can affect the artifact."""
    datasets = doc.get("input") if isinstance(doc.get("input"), dict) else {}
    columns = [c for c in doc.get("columns") or [] if isinstance(c, dict)]
    by_name = {c.get("name"): c for c in columns}
    lookups = {
        m.get("id"): m for m in doc.get("intermediates") or [] if isinstance(m, dict)
    }
    rows = [r for r in doc.get("rows") or [] if isinstance(r, dict)]
    root = bundle.classes["root_class"]
    live = {"variable": set(), "dataset": set(), "lookup": set()}

    def mark(refs):
        grew = False
        for kind, name in refs:
            if kind == "variable" and "." in name:
                kind, name = "dataset", name.split(".", 1)[0]
            if kind == "dataset" and name in lookups:
                kind = "lookup"
            if name not in live[kind]:
                live[kind].add(name)
                grew = True
        return grew

    output = doc.get("output") if isinstance(doc.get("output"), dict) else {}
    output_fields = bundle.classes["output_class"]
    for field in ("columns", "order_by"):
        if field in output:
            mark(
                bundle.references(
                    output[field], output_fields[field]["type"], ("output_class", field)
                )
            )
    mark(
        bundle.references(doc.get("keys"), root["keys"]["type"], ("root_class", "keys"))
    )
    mark(("variable", c.get("name")) for c in columns if "verifications" in c)
    for field in ("verifications", "filter"):
        if field in doc:
            mark(bundle.references(doc[field], root[field]["type"]))
    # REQ-0638: `base`, and the dataset a row falls back to: `base`, or the
    # one input when there is no `base` (REQ-0064).
    driver = doc.get("base")
    if isinstance(driver, str):
        mark([("dataset", driver)])
    elif len(datasets) == 1:
        driver = next(iter(datasets))
    if isinstance(driver, str) and (not rows or any("dataset" not in r for r in rows)):
        mark([("dataset", driver)])
    row_fields = bundle.classes["row_class"]
    for row in rows:
        if isinstance(row.get("dataset"), str):
            mark([("dataset", row["dataset"])])
        for field in ("group_by", "filter"):
            if field in row:
                mark(bundle.references(row[field], row_fields[field]["type"]))

    column_fields = bundle.classes["column_class"]
    lookup_fields = bundle.classes["intermediate_class"]
    done = {"variable": set(), "lookup": set(), "row": set()}
    grew = True
    while grew:
        grew = False
        for name in live["variable"] - done["variable"]:
            done["variable"].add(name)
            column = by_name.get(name) or {}
            for field in ("derivation", "verifications"):
                if field in column:
                    grew |= mark(
                        bundle.references(column[field], column_fields[field]["type"])
                    )
        for i, row in enumerate(rows):
            for target, d in (row.get("derivations") or {}).items():
                if target in live["variable"] and (i, target) not in done["row"]:
                    done["row"].add((i, target))
                    grew |= mark(bundle.references(d, "derivation"))
        for lid in live["lookup"] - done["lookup"]:
            done["lookup"].add(lid)
            lookup = lookups.get(lid) or {}
            if isinstance(lookup.get("dataset"), str):
                grew |= mark([("dataset", lookup["dataset"])])
            for field, value in lookup.items():
                if field not in ("id", "dataset") and field in lookup_fields:
                    grew |= mark(
                        bundle.references(
                            value,
                            lookup_fields[field]["type"],
                            ("intermediate_class", field),
                        )
                    )

    out = dict(doc)
    if "columns" in out and isinstance(out["columns"], list):
        out["columns"] = [c for c in columns if c.get("name") in live["variable"]]
    if isinstance(out.get("input"), dict):
        out["input"] = {k: v for k, v in datasets.items() if k in live["dataset"]}
    if isinstance(out.get("intermediates"), list):
        out["intermediates"] = [
            m for m in lookups.values() if m.get("id") in live["lookup"]
        ]
        if not out["intermediates"]:
            del out["intermediates"]
    dead = set(by_name) - live["variable"]
    for row in rows:
        if isinstance(row.get("derivations"), dict):
            # A row derivation for a dead column goes with it; one naming no
            # declared column stays for the engine to report (REQ-0641).
            row["derivations"] = {
                k: v for k, v in row["derivations"].items() if k not in dead
            }
    return out


def _order_columns(bundle, doc):
    """REQ-0643: each column after every column its derivation reads, ties
    broken by first contribution. An undeclared or cyclic read is left for
    the engine to report; ordering does not repair it."""
    columns = doc.get("columns")
    if not isinstance(columns, list) or not all(isinstance(c, dict) for c in columns):
        return doc
    names = [c.get("name") for c in columns]
    position = {n: i for i, n in enumerate(names)}
    lookups = {
        m.get("id"): m for m in doc.get("intermediates") or [] if isinstance(m, dict)
    }
    lookup_fields = bundle.classes["intermediate_class"]
    rows = [r for r in doc.get("rows") or [] if isinstance(r, dict)]

    def reads(column):
        refs = set()
        if "derivation" in column:
            refs |= bundle.references(column["derivation"], "derivation")
        for row in rows:
            d = (row.get("derivations") or {}).get(column.get("name"))
            if d is not None:
                refs |= bundle.references(d, "derivation")
        deps = set()
        for kind, name in refs:
            if kind != "variable":
                continue
            if "." not in name:
                deps.add(name)
                continue
            lookup = lookups.get(name.split(".", 1)[0]) or {}
            for field in ("key", "between", "filter", "order_by"):
                if field in lookup:
                    deps |= {
                        n
                        for k, n in bundle.references(
                            lookup[field],
                            lookup_fields[field]["type"],
                            ("intermediate_class", field),
                        )
                        if k == "variable" and "." not in n
                    }
        return {n for n in deps if n in position and n != column.get("name")}

    pending = {c["name"]: reads(c) for c in columns}
    ordered = []
    while pending:
        ready = [n for n in names if n in pending and not pending[n] - set(ordered)]
        if not ready:
            ordered.extend(n for n in names if n in pending)
            break
        ordered.append(ready[0])
        del pending[ready[0]]
    by_name = {c["name"]: c for c in columns}
    return {**doc, "columns": [by_name[n] for n in ordered]}


def _schema_order(bundle, doc):
    """REQ-0649: root and member fields in schema order when materialized."""
    out = bundle.ordered(doc, bundle.classes["root_class"])
    for name, (_, class_name) in _KEYED.items():
        fields = bundle.classes[class_name]
        value = out.get(name)
        if isinstance(value, dict):
            out[name] = {
                k: bundle.ordered(m, fields) if isinstance(m, dict) else m
                for k, m in value.items()
            }
        elif isinstance(value, list):
            out[name] = [
                bundle.ordered(m, fields) if isinstance(m, dict) else m for m in value
            ]
    return out


# --------------------------------------------------------------- resolution


class Resolved:
    """A resolved specification (REQ-0645 to REQ-0650).

    `document` is the resolved YAML data tree. `layer_paths` maps the
    logical path of each surviving input `path` or `schema` an inherited
    layer wrote relative to its own directory onto that directory and the
    layer's own spelling, which resource resolution retries (REQ-0780).
    """

    def __init__(self, document, layer_paths):
        self.document = document
        self.layer_paths = layer_paths


def resolve(entry_path, entry_doc=None):
    """Resolve the entry file's `parents` chain (REQ-0614 onward)."""
    entry_path = os.path.realpath(entry_path)
    bundle = load_bundle(_bundle_root(os.path.dirname(entry_path)))
    layers = _linearize(bundle, entry_path, entry_doc)
    resolved, written = _merge(bundle, layers)
    _materialize(bundle, resolved)
    resolved = _expand_windows(bundle, resolved)
    resolved = _prune(bundle, resolved)
    resolved = _order_columns(bundle, resolved)
    resolved = _schema_order(bundle, resolved)
    datasets = resolved.get("input") if isinstance(resolved.get("input"), dict) else {}
    layer_paths = {
        logical: origin
        for logical, origin in written.items()
        if logical.split(".")[1] in datasets
    }
    return Resolved(resolved, layer_paths)


def _inside(root, path):
    return os.path.commonpath([root, path]) == root


def layer_path(engine, logical, path):
    """The spelling resource resolution reads an inherited input path by.

    REQ-0780/REQ-0781: the rebased path names the file from the writing
    layer's directory first. When nothing is stored there, the layer's own
    spelling is retried against the project root (REQ-1246); a location a
    layer outside the project root names is not an anchor at all. Any other
    outcome -- a stored entry, or a traversal out of the project root from a
    layer inside it -- is terminal and reads the rebased path.
    """
    origin = engine.layer_paths.get(logical)
    if origin is None:
        return path
    layer_dir, spelling = origin
    root = os.path.realpath(engine.spec_dir)
    full = os.path.normpath(os.path.join(root, path))
    retry = os.path.normpath(os.path.join(root, spelling))
    if _inside(root, full):
        if os.path.lexists(full) or not (
            _inside(root, retry) and os.path.lexists(retry)
        ):
            return path
        return spelling
    return path if _inside(root, layer_dir) else spelling
