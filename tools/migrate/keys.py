"""Today's inferred join keys, written out as 1.0 declarations (D23, D63).

Under 0.5 an implicit join, and a named intermediate or qualified aggregate
without `key`, match on the applicable keys: the output `keys`, in key
order, that the lookup's relation carries (LOOKUP-1, LOOKUP-3, LOOKUP-5).
1.0 declares them: `key` on every input that an implicit join or a keyless
qualified aggregate reads (P-JOIN-1, P-KEY-1), and on every named
intermediate except one that drives a row template (D23-R1). An input that an
intermediate only renames declares that intermediate's key too, so LOOKUP-4
still refuses the alias (D71).

For an entry specification composed with its parents (LAYER-5, LAYER-6),
this module classifies each qualified read outside a lookup's own fields as
a driver read, a join or an intermediate read, collects the qualified
aggregates, and reads each relation's fields from its CSV header, Parquet
schema or `schema` producer, as the method page of the RFC describes.
"""

import csv
import functools
import re
from dataclasses import dataclass, field
from pathlib import Path

import yaml
from tools.migrate import schema
from tools.migrate.text import compose, value_of

IDENTITY = {"intermediates": "id", "columns": "name", "rows": "id"}
KEYED_MAPS = ("input", "windows")
# A qualified name, or a reduction's whole record, `DATASET.*` (FORMULA-18).
QUALIFIED = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:(?:\.[A-Za-z_][A-Za-z0-9_]*)+|\.\*)")
NAME_TYPES = frozenset(
    {
        "variable",
        "derivation",
        "case_result",
        "filtered_source",
        "order_by_term",
        "untyped_intermediate_derivation",
        "intermediate_derivation",
        "function_arg",
    }
)
EXPRESSION_TYPES = frozenset(
    {"predicate", "numeric_expression", "aggregate_expression"}
)
# Fields of a lookup that read its own relation, the current row's match
# values or the row's driver; none of them is a join (LOOKUP-1, LOOKUP-9).
LOOKUP_FIELDS = frozenset(
    {
        "source_binding_class.filter",
        "source_binding_class.order_by",
        "source_binding_class.keep",
        "source_binding_class.between",
        "source_binding_class.absent",
        "source_binding_class.require_match",
    }
)


class LayerError(ValueError):
    """A layer chain that cannot be composed; `path` names the offending file."""

    def __init__(self, reason, path):
        super().__init__(reason)
        self.reason = reason
        self.path = Path(path)

    def describe(self, base):
        try:
            where = self.path.resolve().relative_to(Path(base).resolve()).as_posix()
        except ValueError:
            where = self.path.name
        return f"{self.reason} {where}"


def load_layer(path, cache):
    path = Path(path).resolve()
    if path not in cache:
        text = path.read_text(encoding="utf-8")
        root = compose(text)
        cache[path] = (text, root, value_of(root) if root is not None else None)
    return cache[path]


def chain(entry, cache):
    """The layers of a specification, root ancestor first (LAYER-1 to LAYER-3)."""
    layers = []
    seen = set()
    current = Path(entry).resolve()
    while True:
        if current in seen:
            raise LayerError("cyclic parent chain at", current)
        seen.add(current)
        if not current.is_file():
            raise LayerError("missing layer", current)
        _, _, value = load_layer(current, cache)
        if not isinstance(value, dict):
            raise LayerError("not a mapping:", current)
        layers.append((current, value))
        parent = value.get("parent")
        if not isinstance(parent, str):
            break
        current = (current.parent / parent).resolve()
    layers.reverse()
    return layers


@dataclass
class Composed:
    """An entry specification composed with its parents, with provenance."""

    entry: Path
    value: dict
    layers: list
    declarers: dict = field(default_factory=dict)
    path_bases: dict = field(default_factory=dict)


@functools.cache
def _registry_keywords():
    return {
        name: set(registry.keywords)
        for name, registry in schema.bundle("spec").registries.items()
    }


def _different_keywords(old, new):
    """True when two one-key mappings name different keywords of one registry."""
    if len(old) != 1 or len(new) != 1 or set(old) == set(new):
        return False
    (first,), (second,) = old, new
    return any(
        first in keywords and second in keywords
        for keywords in _registry_keywords().values()
    )


def _merge(old, new):
    """LAYER-6: classes and mappings merge key by key, a registry value only
    under the same keyword, and everything else is replaced."""
    if new is None:
        return None
    if isinstance(old, dict) and isinstance(new, dict):
        if _different_keywords(old, new):
            return dict(new)
        out = dict(old)
        for key, value in new.items():
            if value is None:
                out.pop(key, None)
            elif key in out:
                out[key] = _merge(out[key], value)
            else:
                out[key] = value
        return out
    return new


def compose_layers(layers, entry):
    """Compose layer values under LAYER-5 and LAYER-6; YAML null removes (LAYER-9)."""
    result = {}
    declarers = {}
    bases = {}
    entry_dir = Path(entry).resolve().parent
    for path, layer in layers:
        for name, value in layer.items():
            if name in ("schema_version", "parent"):
                continue
            if value is None:
                result.pop(name, None)
                continue
            if name in KEYED_MAPS and isinstance(value, dict):
                merged = dict(result.get(name) or {})
                for member_name, member in value.items():
                    if member is None:
                        merged.pop(member_name, None)
                        continue
                    if name == "input" and isinstance(member, str):
                        member = {"path": member}
                    declarers.setdefault((name, member_name), path)
                    if name == "input" and isinstance(member, dict):
                        for written in ("path", "schema"):
                            if written in member:
                                bases[(member_name, written)] = path.parent
                    if member_name in merged:
                        merged[member_name] = _merge(merged[member_name], member)
                    else:
                        merged[member_name] = member
                result[name] = merged
            elif name in IDENTITY and isinstance(value, list):
                identity = IDENTITY[name]
                merged = list(result.get(name) or [])
                index = {
                    item.get(identity): position
                    for position, item in enumerate(merged)
                    if isinstance(item, dict)
                }
                for member in value:
                    member_id = (
                        member.get(identity) if isinstance(member, dict) else None
                    )
                    if member_id is not None:
                        declarers.setdefault((name, member_id), path)
                    if member_id is not None and member_id in index:
                        merged[index[member_id]] = _merge(
                            merged[index[member_id]], member
                        )
                    else:
                        if member_id is not None:
                            index[member_id] = len(merged)
                        merged.append(member)
                result[name] = merged
            else:
                result[name] = value
    for name, dataset in (result.get("input") or {}).items():
        if isinstance(dataset, dict) and dataset.get("relative_to") == "entry":
            for written in ("path", "schema"):
                bases[(name, written)] = entry_dir
    return result, declarers, bases


def composed(entry, cache):
    layers = chain(entry, cache)
    value, declarers, bases = compose_layers(layers, entry)
    return Composed(Path(entry).resolve(), value, layers, declarers, bases)


# -- relation fields -----------------------------------------------------------


def csv_header(path):
    """The fields of a CSV file's header record; only that record is decoded."""
    with open(path, "rb") as handle:
        first = handle.readline()
    text = first.decode("utf-8").rstrip("\r\n")
    for row in csv.reader([text]):
        return row
    return []


def parquet_names(path):
    import pyarrow.parquet as parquet

    return list(parquet.read_schema(path).names)


class Relations:
    """Reads the stored fields of an input, once per file."""

    def __init__(self, cache):
        self.cache = cache
        self.files = {}

    def fields(self, spec, name):
        """The fields of input `name`, or (None, the reason they are unknown)."""
        dataset = (spec.value.get("input") or {}).get(name)
        if isinstance(dataset, str):
            dataset = {"path": dataset}
        if not isinstance(dataset, dict):
            return None, f"input {name} is not declared"
        extra = [dataset["ordinal"]] if isinstance(dataset.get("ordinal"), str) else []
        producer = dataset.get("schema")
        if isinstance(producer, str):
            base = spec.path_bases.get((name, "schema"), spec.entry.parent)
            location = (base / producer).resolve()
            try:
                produced = composed(location, self.cache)
            except LayerError as error:
                return None, f"schema producer {producer} not read: {error.reason}"
            except (OSError, yaml.YAMLError) as error:
                return (
                    None,
                    f"schema producer {producer} not read: {type(error).__name__}",
                )
            columns = ((produced.value.get("output") or {}).get("columns")) or []
            return [str(column) for column in columns] + extra, None
        path = dataset.get("path")
        if not isinstance(path, str):
            return None, f"input {name} declares no path"
        base = spec.path_bases.get((name, "path"), spec.entry.parent)
        location = (base / path).resolve()
        suffix = location.suffix.lower()
        if location not in self.files:
            try:
                if suffix == ".csv":
                    self.files[location] = csv_header(location)
                elif suffix == ".parquet":
                    self.files[location] = parquet_names(location)
                else:
                    self.files[location] = None
            except (OSError, UnicodeError, ImportError, ValueError) as error:
                self.files[location] = ValueError(type(error).__name__)
        found = self.files[location]
        if found is None:
            return None, f"input {name} ({path}) is not a CSV or Parquet file"
        if isinstance(found, Exception):
            return None, f"input {name} ({path}) not read: {found}"
        return list(found) + extra, None


# -- reads ---------------------------------------------------------------------


def _role(bundle, type_expr):
    if isinstance(type_expr, str):
        if type_expr in NAME_TYPES:
            return "name"
        if type_expr in EXPRESSION_TYPES:
            return "expression"
        if type_expr == "string_template":
            return "template"
        if type_expr == "odm_item":
            return "odm"
        expanded = bundle.expand(type_expr)
        if expanded is not type_expr and expanded != type_expr:
            return _role(bundle, expanded)
        return None
    if isinstance(type_expr, tuple) and type_expr[0] == "union":
        for option in type_expr[1]:
            role = _role(bundle, option)
            if role:
                return role
    return None


def _strip_quoted(text):
    return re.sub(r"'(?:[^']|'')*'", "''", text)


def qualifiers(text, role):
    """The qualifiers of the qualified names a scalar reads."""
    if role == "name":
        return [text.split(".", 1)[0]] if "." in text else []
    if role == "template":
        names = re.findall(r"(?<!\{)\{([^{}]+)\}(?!\})", text)
        return [name.strip().split(".", 1)[0] for name in names if "." in name]
    if role == "expression":
        return [
            match.group(0).split(".", 1)[0]
            for match in QUALIFIED.finditer(_strip_quoted(text))
        ]
    return []


@dataclass
class Aggregate:
    context: tuple
    datasets: set
    keyed: bool
    path: tuple


@dataclass
class Read:
    context: tuple
    qualifier: str
    path: tuple


def _excluded(visit):
    """True inside a lookup's own fields, a window, an odm read or a key."""
    current = visit
    while current is not None:
        location = current.location()
        if location in LOOKUP_FIELDS:
            return True
        if (
            current.kind == "keyword"
            and current.name in ("odm",)
            and current.owner == "expressions"
        ):
            return True
        if location and (
            location.startswith("window_spec.")
            or location.startswith("record_between_class.")
        ):
            return True
        if (
            current.kind == "field"
            and current.name in ("window", "key")
            and current.owner not in ("root_class",)
        ):
            return True
        current = current.holder
    return False


def _inside_aggregate(visit):
    current = visit.holder
    while current is not None:
        if (
            current.kind == "keyword"
            and current.name == "aggregate"
            and current.owner == "expressions"
        ):
            return True
        current = current.holder
    return False


def _context(path):
    if not path:
        return None
    if path[0] == "columns" and len(path) > 2 and path[2] == "derivation":
        return ("column", path[1])
    if path[0] == "rows" and len(path) > 2 and path[2] in ("derivations", "filter"):
        return ("row", path[1])
    if path == ("filter",):
        return ("base",)
    if path[0] == "verifications":
        return ("verification",)
    return None


def collect(bundle, root):
    """The qualified reads and aggregates of a composed specification's node tree."""
    reads = []
    aggregates = []
    for visit in bundle.walk(root, bundle.root):
        context = _context(visit.path)
        if context is None:
            continue
        if (
            visit.kind == "keyword"
            and visit.name == "aggregate"
            and visit.owner == "expressions"
        ):
            if _inside_aggregate(visit):
                continue
            aggregates.append(_aggregate(bundle, visit, context))
            continue
        if not isinstance(visit.value, yaml.ScalarNode) or _inside_aggregate(visit):
            continue
        if _excluded(visit):
            continue
        role = _role(bundle, visit.type)
        if role in (None, "odm"):
            continue
        for qualifier in qualifiers(visit.value.value, role):
            reads.append(Read(context, qualifier, visit.path))
    return reads, aggregates


def _aggregate(bundle, visit, context):
    payload = visit.value
    datasets = set()
    keyed = False
    if isinstance(payload, yaml.ScalarNode):
        datasets.update(qualifiers(payload.value, "expression"))
    elif isinstance(payload, yaml.MappingNode):
        for inner in bundle.walk(payload, visit.type, visit.path, visit):
            if (
                inner.kind == "field"
                and inner.owner == "aggregate_class"
                and inner.name == "key"
            ):
                keyed = True
                continue
            if not isinstance(inner.value, yaml.ScalarNode):
                continue
            if _within(inner, "aggregate_class", "key") or _within(
                inner, "aggregate_class", "between"
            ):
                continue
            role = _role(bundle, inner.type)
            if role in (None, "odm"):
                continue
            datasets.update(qualifiers(inner.value.value, role))
    return Aggregate(context, datasets, keyed, visit.path)


def _within(visit, owner, name):
    current = visit
    while current is not None:
        if current.kind == "field" and current.owner == owner and current.name == name:
            return True
        current = current.holder
    return False


# -- inference -----------------------------------------------------------------


@dataclass
class Inference:
    intermediate_keys: dict = field(default_factory=dict)
    intermediate_drivers: list = field(default_factory=list)
    input_keys: dict = field(default_factory=dict)
    joins: dict = field(default_factory=dict)
    aggregate_inputs: set = field(default_factory=set)
    aggregate_keys: list = field(default_factory=list)
    problems: list = field(default_factory=list)


def _drivers(value):
    inputs = list((value.get("input") or {}).keys())
    rows = value.get("rows")
    if isinstance(rows, list) and rows:
        drivers = []
        for row in rows:
            dataset = row.get("dataset") if isinstance(row, dict) else None
            if dataset is None and len(inputs) == 1:
                dataset = inputs[0]
            drivers.append(dataset)
        return drivers, None
    base = value.get("base")
    if base is None and len(inputs) == 1:
        base = inputs[0]
    return None, base


def drivers_of(value):
    """The inputs and intermediates that drive a composed specification's rows."""
    drivers, base = _drivers(value)
    return {driver for driver in (drivers or [base]) if driver}


def infer(spec, relations, bundle=None):
    """Infer the keys today's engine applies to an entry specification."""
    bundle = bundle or schema.bundle("spec")
    value = spec.value
    result = Inference()
    keys = [str(key) for key in value.get("keys") or []]
    inputs = value.get("input") or {}
    intermediates = [
        item for item in value.get("intermediates") or [] if isinstance(item, dict)
    ]
    intermediate_ids = {item.get("id") for item in intermediates}
    rows = value.get("rows") if isinstance(value.get("rows"), list) else []
    drivers, base = _drivers(value)
    columns = [item for item in value.get("columns") or [] if isinstance(item, dict)]

    if len(spec.layers) == 1:
        node = load_layer(spec.layers[0][0], relations.cache)[1]
    else:
        node = compose(yaml.safe_dump(value, sort_keys=False))
    reads, aggregates = collect(bundle, node)

    def context_drivers(context):
        kind = context[0]
        if drivers is None:
            return {base}
        if kind == "row":
            return {drivers[context[1]]} if context[1] < len(drivers) else set()
        if kind == "column":
            column = columns[context[1]] if context[1] < len(columns) else {}
            name = column.get("name")
            result_ = set()
            for row, driver in zip(rows, drivers):
                derived = (
                    (row.get("derivations") or {}) if isinstance(row, dict) else {}
                )
                if name not in derived:
                    result_.add(driver)
            return result_
        if kind == "base":
            return {base}
        return set(drivers)

    def applicable(name):
        fields, problem = relations.fields(spec, name)
        if fields is None:
            return None, problem
        return [key for key in keys if key in fields], None

    joined = {}
    for read in reads:
        qualifier = read.qualifier
        if (
            qualifier in intermediate_ids
            or qualifier == "SELF"
            or qualifier not in inputs
        ):
            continue
        active = context_drivers(read.context)
        if not active or all(driver == qualifier for driver in active):
            continue
        joined.setdefault(qualifier, []).append(read)
    for name, found in joined.items():
        result.joins[name] = found

    for aggregate in aggregates:
        datasets = {item for item in aggregate.datasets if item in inputs}
        if len(datasets) != 1 or aggregate.keyed:
            continue
        (name,) = datasets
        if aggregate.context[0] == "row":
            index = aggregate.context[1]
            row = rows[index] if index < len(rows) else {}
            if isinstance(row, dict) and row.get("group_by") and drivers[index] == name:
                continue
        result.aggregate_inputs.add(name)
        result.aggregate_keys.append((name, aggregate.path))

    for name in sorted(
        set(result.joins) | result.aggregate_inputs, key=list(inputs).index
    ):
        found, problem = applicable(name)
        if found is None:
            result.problems.append(("input", name, problem))
        elif not found:
            result.problems.append(
                ("input", name, f"input {name} carries none of the output keys {keys}")
            )
        else:
            result.input_keys[name] = found

    row_drivers = {row.get("dataset") for row in rows if isinstance(row, dict)}
    for item in intermediates:
        identifier = item.get("id")
        if "key" in item:
            continue
        if identifier in row_drivers:
            result.intermediate_drivers.append(identifier)
            continue
        dataset = item.get("dataset")
        if dataset == "SELF":
            fields = [column.get("name") for column in columns]
            problem = None
        elif dataset in inputs:
            fields, problem = relations.fields(spec, dataset)
            if fields is not None:
                fields = fields + list((item.get("derivations") or {}).keys())
        else:
            fields, problem = (
                None,
                f"intermediate {identifier} reads {dataset}, which is not an input",
            )
        if fields is None:
            result.problems.append(("intermediate", identifier, problem))
            continue
        found = [key for key in keys if key in fields]
        if not found:
            result.problems.append(
                (
                    "intermediate",
                    identifier,
                    f"{dataset} carries none of the output keys {keys}",
                )
            )
            continue
        result.intermediate_keys[identifier] = found
    # A 1.0 intermediate only renames its input when its key lists the
    # input's declared key (LOOKUP-4, D71), so the input it renames declares
    # the same key and the refusal stays.
    for item in intermediates:
        dataset = item.get("dataset")
        found = result.intermediate_keys.get(item.get("id"))
        if found and dataset in inputs and dataset not in result.input_keys:
            if renames_only(item):
                result.input_keys[dataset] = found
    return result


def renames_only(item):
    """True when a 0.5 intermediate declares nothing beyond `id`, `dataset`,
    a missing `no_match` and `require_match: false`, which LOOKUP-4 refuses
    as an alias of its input."""
    if not set(item) <= {"id", "dataset", "no_match", "require_match"}:
        return False
    return item.get("require_match") in (None, False) and item.get(
        "no_match", {"literal": None}
    ) == {"literal": None}
