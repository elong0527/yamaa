"""Compatibility shims mapping the clean-room yamaa package onto the names
`.github/scripts/yaml-validation/validate_repository.py` read from the old
package (`yamaa.regex`, `yamaa.expressions.predicates`, `yamaa.io.csv`,
`yamaa.models`). The shims preserve the validation outcomes; they are local
to the yaml-validation scripts and are not part of the yamaa package.
"""

import copy as _copy
import os as _os
import re as _re
import sys as _sys

_sys.path.insert(
    0,
    _os.path.join(
        _os.path.dirname(
            _os.path.dirname(
                _os.path.dirname(_os.path.dirname(_os.path.abspath(__file__)))
            )
        ),
        "python",
        "src",
    ),
)

from yamaa import csv_io as _csv_io
from yamaa import pred as _pred
from yamaa import validate as _validate
from yamaa import values as _values
from yamaa.errors import YamaaError as _YamaaError


# ---------------------------------------------------------------------------
# yamaa.regex -> clean-room R022 portable-pattern check + stdlib re
# ---------------------------------------------------------------------------
class RegexError(ValueError):
    """A pattern the R022 portable contract rejects."""

    condition = "invalid_regex"

    def __init__(self, pattern, reason):
        super().__init__(f"the R022 contract rejects {pattern!r}: {reason}")
        self.pattern = pattern
        self.reason = reason


def _rejected(pattern):
    """Return the rejection reason, or None when the pattern is portable."""
    try:
        _validate._check_portable_pattern(pattern, "<regex>")
    except _YamaaError as exc:
        return str(exc)
    return None


def compile_pattern(pattern):
    """Compile one pattern through the portable contract."""
    reason = _rejected(pattern)
    if reason is not None:
        raise RegexError(pattern, reason)
    try:
        # The R022 contract pins ASCII semantics for \d \w \b (REQ-0822),
        # so re-compile the normalized source with re.ASCII.
        normalized = _validate._normalize_pattern(pattern).pattern
        return _re.compile(normalized, _re.ASCII)
    except _re.error as error:
        raise RegexError(pattern, str(error)) from error


def full_match(pattern, subject):
    """Return whether the pattern accepts the whole subject."""
    return compile_pattern(pattern).fullmatch(subject) is not None


def capture_group_count(pattern):
    """Count the capturing groups an accepted pattern declares."""
    return compile_pattern(pattern).groups


class regex:
    RegexError = RegexError
    compile_pattern = staticmethod(compile_pattern)
    full_match = staticmethod(full_match)
    capture_group_count = staticmethod(capture_group_count)


# ---------------------------------------------------------------------------
# yamaa.expressions.predicates -> yamaa.pred
# ---------------------------------------------------------------------------
class PredicateError(ValueError):
    """A predicate expression the grammar rejects."""

    condition = "invalid_predicate"

    def __init__(self, message, position=0):
        super().__init__(f"{message} at character {position + 1}")
        self.position = position


COMPARISON_OPERATORS = ("=", "<>", "<", "<=", ">", ">=")
RESERVED_NAMES = frozenset(_pred.KEYWORDS)


def _dangling_escape(pattern, escape):
    escaped = False
    for character in pattern:
        if escaped:
            escaped = False
        elif character == escape:
            escaped = True
    return escaped


def _to_validator_ast(node):
    """Convert the clean-room tuple AST to the validator's dict AST shape."""
    tag = node[0]
    if tag == "or":
        return {
            "kind": "or",
            "left": _to_validator_ast(node[1]),
            "right": _to_validator_ast(node[2]),
        }
    if tag == "and":
        return {
            "kind": "and",
            "left": _to_validator_ast(node[1]),
            "right": _to_validator_ast(node[2]),
        }
    if tag == "not":
        inner = node[1]
        # NOT IN / NOT BETWEEN / NOT LIKE fold into the negated node.
        if isinstance(inner, tuple) and inner[0] in {"in", "between", "like"}:
            converted = _to_validator_ast(inner)
            converted["negated"] = True
            return converted
        return {"kind": "not", "value": _to_validator_ast(inner)}
    if tag == "cmp":
        return {
            "kind": "comparison",
            "operator": node[1],
            "left": _to_validator_ast(node[2]),
            "right": _to_validator_ast(node[3]),
        }
    if tag == "isnull":
        return {
            "kind": "null_test",
            "value": _to_validator_ast(node[1]),
            "negated": False,
        }
    if tag == "isnotnull":
        return {
            "kind": "null_test",
            "value": _to_validator_ast(node[1]),
            "negated": True,
        }
    if tag == "in":
        return {
            "kind": "in",
            "value": _to_validator_ast(node[1]),
            "values": [_to_validator_ast(v) for v in node[2]],
            "negated": False,
        }
    if tag == "between":
        return {
            "kind": "between",
            "value": _to_validator_ast(node[1]),
            "lower": _to_validator_ast(node[2]),
            "upper": _to_validator_ast(node[3]),
            "negated": False,
        }
    if tag == "like":
        return {
            "kind": "like",
            "value": _to_validator_ast(node[1]),
            "pattern": _to_validator_ast(node[2]),
            "escape": node[3],
            "negated": False,
        }
    if tag == "lit":
        value = node[1]
        raw = node[2] if len(node) > 2 else None
        if isinstance(value, bool):
            return {"kind": "boolean", "value": value}
        if value is None:
            return {"kind": "literal", "type": None, "value": None, "position": 0}
        if isinstance(value, int):
            return {
                "kind": "literal",
                "type": "int",
                "value": raw if raw is not None else str(value),
                "position": 0,
            }
        if isinstance(value, float):
            return {
                "kind": "literal",
                "type": "float",
                "value": raw if raw is not None else repr(value),
                "position": 0,
            }
        if isinstance(value, _values.YDateTime):
            return {
                "kind": "literal",
                "type": "datetime",
                "value": raw if raw is not None else _values.datetime_text(value),
                "position": 0,
            }
        if isinstance(value, _values.YDate):
            return {
                "kind": "literal",
                "type": "date",
                "value": raw if raw is not None else _values.date_text(value),
                "position": 0,
            }
        return {"kind": "literal", "type": "str", "value": value, "position": 0}
    if tag == "ident":
        return {"kind": "identifier", "name": node[1], "position": 0}
    if tag == "str_contains":
        # REQ-0162/REQ-1241: str_contains(source, pattern) is the one Boolean
        # function call the predicate grammar admits. The clean-room parser
        # emits ("str_contains", source, pattern) with the pattern as its
        # raw validated text; the validator only needs the source operand
        # for its type check (the pattern was already validated by the
        # engine parser before this converter runs).
        # Check the function name carried by the node itself (not just the
        # dispatch tag) and echo it into the output: a future second Boolean
        # call in the predicate grammar must raise PredicateError here,
        # never be mislabeled "str_contains".
        if node[0] != "str_contains":
            raise PredicateError(f"unsupported predicate call: {node[0]!r}")
        try:
            _, source, pattern = node
        except (TypeError, ValueError):
            raise PredicateError(f"malformed predicate call node: {node!r}")
        if not isinstance(pattern, str):
            raise PredicateError(f"malformed predicate call node: {node!r}")
        return {
            "kind": "call",
            "name": node[0],
            "source": _to_validator_ast(source),
            "pattern": pattern,
        }
    raise PredicateError(f"unsupported predicate node: {tag!r}")


def _check_dangling_escapes(node):
    """Reject a LIKE pattern with a dangling escape, like the old grammar."""
    kind = node[0]
    if kind == "like":
        _check_dangling_escapes(node[1])
        _check_dangling_escapes(node[2])
        _pattern, _escape = node[2], node[3]
        if (
            _escape is not None
            and _pattern[0] == "lit"
            and isinstance(_pattern[1], str)
            and _dangling_escape(_pattern[1], _escape)
        ):
            raise PredicateError("LIKE pattern has a dangling escape")
    elif kind in ("or", "and"):
        _check_dangling_escapes(node[1])
        _check_dangling_escapes(node[2])
    elif kind == "not":
        _check_dangling_escapes(node[1])
    elif kind == "cmp":
        _check_dangling_escapes(node[2])
        _check_dangling_escapes(node[3])
    elif kind in ("isnull", "isnotnull"):
        _check_dangling_escapes(node[1])
    elif kind == "in":
        _check_dangling_escapes(node[1])
        for _v in node[2]:
            _check_dangling_escapes(_v)
    elif kind == "between":
        _check_dangling_escapes(node[1])
        _check_dangling_escapes(node[2])
        _check_dangling_escapes(node[3])


def parse_predicate(text):
    """Parse one predicate expression, raising PredicateError when rejected."""
    if not isinstance(text, str) or not text:
        raise PredicateError("predicate must be a non-empty string")
    try:
        node, _names = _pred.parse(text, "<predicate>")
    except _YamaaError as exc:
        raise PredicateError(str(exc)) from exc
    _check_dangling_escapes(node)
    return _to_validator_ast(node)


class predicates:
    PredicateError = PredicateError
    COMPARISON_OPERATORS = COMPARISON_OPERATORS
    RESERVED_NAMES = RESERVED_NAMES
    parse_predicate = staticmethod(parse_predicate)


# ---------------------------------------------------------------------------
# yamaa.io.csv -> yamaa.csv_io
# ---------------------------------------------------------------------------
class CsvProfileFailure(ValueError):
    """One R023 ingest failure at a source coordinate."""

    def __init__(self, condition, record, field):
        self.condition = condition
        self.record = record
        self.field = field
        super().__init__(f"{condition} at record {record}, field {field!r}")


def scan_records(data):
    """Scan decoded source text into records of text or missing (None)."""
    try:
        records = _csv_io._scan_records(data, "<csv>", None, "<csv>", "<csv>")
    except _YamaaError as exc:
        condition = exc.condition
        record = exc.context.get("record", 0)
        field = exc.context.get("field", 0)
        raise CsvProfileFailure(condition, record, field) from exc
    # The old contract spells a missing field None; the engine reads "".
    return [[f if f != "" else None for f in rec] for rec in records]


def render_records(names, records):
    """Render a header and records to the exact R020 bytes."""
    fields = [n if n is not None else "" for n in names]
    lines = [_csv_io._csv_record(fields)]
    for rec in records:
        lines.append(_csv_io._csv_record([f if f is not None else "" for f in rec]))
    return ("".join(line + "\n" for line in lines)).encode("utf-8")


def fixed_point(value, decimals):
    """Round one binary64 value to `decimals` places, tie away from zero."""
    return _csv_io._fixed_point(value, decimals)


def record_coordinates(prefix):
    """The record and field a reader stands at after reading `prefix`."""
    record = 1
    field = 1
    index = 0
    while index < len(prefix):
        character = prefix[index]
        if character == '"':
            index += 1
            while index < len(prefix):
                if prefix[index] == '"':
                    if prefix[index + 1 : index + 2] == '"':
                        index += 2
                        continue
                    index += 1
                    break
                index += 1
            continue
        if character == ",":
            field += 1
        elif character == "\n":
            record += 1
            field = 1
        index += 1
    return record, field


class csv:
    CsvProfileFailure = CsvProfileFailure
    scan_records = staticmethod(scan_records)
    render_records = staticmethod(render_records)
    fixed_point = staticmethod(fixed_point)
    record_coordinates = staticmethod(record_coordinates)


# ---------------------------------------------------------------------------
# yamaa.models -> yamaa.values
# ---------------------------------------------------------------------------
class DateValue(_values.YDate):
    """A strict YYYY-MM-DD calendar date."""

    @classmethod
    def parse(cls, text):
        return cls.parse_date(text)

    @classmethod
    def parse_date(cls, text):
        d = _values.parse_date(text)
        return cls(d.year, d.month, d.day)

    def to_text(self):
        return _values.date_text(self)


class DateTimeValue(_values.YDateTime):
    """A strict YYYY-MM-DDThh:mm:ss civil datetime."""

    @classmethod
    def parse(cls, text):
        return cls.parse_datetime(text)

    @classmethod
    def parse_datetime(cls, text):
        d = _values.parse_datetime(text)
        return cls(d.year, d.month, d.day, d.hour, d.minute, d.second)

    def to_text(self):
        return _values.datetime_text(self)


class _Converted:
    def __init__(self, value):
        self.value = value


def convert_value(value, target):
    """Convert one number to its shortest round-trip text form."""
    if target == "str":
        return _Converted(_values.float_text(value))
    raise ValueError(f"unsupported conversion target: {target!r}")


class models:
    DateValue = DateValue
    DateTimeValue = DateTimeValue
    convert_value = staticmethod(convert_value)


# ---------------------------------------------------------------------------
# yamaa.schema.row_catalog / yamaa.schema.windows /
# yamaa.specification.diagnostics / yamaa.specification.schema
# ---------------------------------------------------------------------------
#
# The repository validator reuses the runtime's spec-shape expansion so its
# paths and reference walkers agree with engine diagnostics. The clean-room
# engine performs row-catalog and named-window expansion internally
# (Engine._expand_catalogs / Engine._expand_named_windows); the shims below
# drive that same machinery over a caller-owned spec dict and translate
# YamaaError into the diagnostic shape the validator reads.
from yamaa import engine as _engine


class Diagnostic:
    """The old engine's ValidationDiagnostic shape the validator reads."""

    def __init__(self, condition, spec_paths, requirement, context):
        self.condition = condition
        self.spec_paths = list(spec_paths)
        self.requirement = requirement
        self.context = context or {}


class SpecificationError(ValueError):
    """The old engine's SpecificationError: carries .diagnostics."""

    def __init__(self, diagnostics):
        self.diagnostics = list(diagnostics)
        super().__init__(
            "; ".join(
                d.condition + "".join(f"@{p}" for p in d.spec_paths)
                for d in self.diagnostics
            )
        )


def _to_diagnostic(error):
    return Diagnostic(
        condition=error.condition,
        spec_paths=error.spec_paths,
        requirement=error.requirement,
        context=error.context,
    )


def _expansion_engine(spec, spec_path):
    """A bare Engine shell: spec + spec_dir only, no construction side
    effects. The expansion methods read nothing else."""
    engine = _engine.Engine.__new__(_engine.Engine)
    engine.spec = spec
    engine.spec_dir = _os.path.dirname(_os.path.abspath(spec_path))
    return engine


def _index_column_paths(spec, paths):
    """The old engine reported column diagnostics as columns[<i>].... The
    clean-room reports them as columns.<name>..... Translate the prefix so
    the validator's manifest paths match; every other path passes through
    unchanged."""
    index = {}
    for i, column in enumerate(spec.get("columns") or []):
        if isinstance(column, dict) and column.get("name") is not None:
            index.setdefault(column["name"], i)
    out = []
    for path in paths:
        match = _re.match(r"columns\.([^.[]+)\.(.*)$", path)
        if match and match.group(1) in index:
            out.append(f"columns[{index[match.group(1)]}].{match.group(2)}")
        else:
            out.append(path)
    return out


def _run_expansion(method, spec, spec_path):
    # The old engine deep-copied the document before expanding; the
    # clean-room expands in place. Copy here so callers keep their
    # authoring dict (a validator unit test reads the catalog entry
    # after expansion).
    engine = _expansion_engine(_copy.deepcopy(spec), spec_path)
    try:
        method(engine)
    except _YamaaError as error:
        diagnostic = _to_diagnostic(error)
        diagnostic.spec_paths = _index_column_paths(spec, diagnostic.spec_paths)
        raise SpecificationError([diagnostic]) from error
    return engine.spec


def expand_row_catalogs(spec, spec_path):
    """yamaa.schema.row_catalog.expand_row_catalogs over the clean-room
    engine: expand catalog templates in place, raising SpecificationError
    on a malformed catalog."""
    return _run_expansion(_engine.Engine._expand_catalogs, spec, spec_path)


def _unknown_window_diagnostics(spec):
    """One unknown_window diagnostic per unresolvable `window:` name
    reference, mirroring the old engine's collecting walk (the
    clean-room raises at the first unknown instead)."""
    defs = spec.get("windows") or {}
    found = []

    def walk(node, where):
        if isinstance(node, dict):
            for k, v in node.items():
                if k == "window" and isinstance(v, str):
                    if v not in defs:
                        found.append(
                            Diagnostic(
                                "unknown_window",
                                [f"{where}.window"],
                                "REQ-1253",
                                {"window": v},
                            )
                        )
                else:
                    walk(v, f"{where}.{k}" if where else str(k))
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, f"{where}[{i}]")

    for c in spec.get("columns") or []:
        if isinstance(c, dict):
            walk(c.get("derivation"), f"columns.{c.get('name')}.derivation")
    for t in spec.get("rows") or []:
        if isinstance(t, dict):
            for dn, dd in (t.get("derivations") or {}).items():
                walk(dd, f"rows.{t.get('id')}.derivations.{dn}")
    for i, im in enumerate(spec.get("intermediates") or []):
        if isinstance(im, dict):
            for dn, dd in (im.get("derivations") or {}).items():
                walk(dd, f"intermediates[{i}].derivations.{dn}")
    for diagnostic in found:
        diagnostic.spec_paths = _index_column_paths(spec, diagnostic.spec_paths)
    return found


def expand_named_windows(spec, bundle=None, strict=True, provenance=None):
    """yamaa.schema.windows.expand_named_windows over the clean-room
    engine. The old SchemaBundle carried the schema file the runtime
    validated against; the clean-room validates window definitions
    inline (REQ-1251), so the bundle is accepted and ignored. Like the
    old engine, the windows block is dropped after expansion when
    strict is true."""
    unknowns = _unknown_window_diagnostics(spec)
    if unknowns:
        raise SpecificationError(unknowns)
    result = _run_expansion(_engine.Engine._expand_named_windows, spec, "<spec>")
    if strict:
        result.pop("windows", None)
    return result


class SchemaBundle:
    """yamaa.specification.schema.SchemaBundle: accepted for signature
    compatibility; the clean-room engine needs no schema bundle."""

    def __init__(
        self, version="1.0", path=None, classes=None, aliases=None, registries=None
    ):
        self.version = version
        self.path = path
        self.classes = classes or {}
        self.aliases = aliases or {}
        self.registries = registries or {}


class row_catalog:
    expand_row_catalogs = staticmethod(expand_row_catalogs)


class windows:
    expand_named_windows = staticmethod(expand_named_windows)


class schema:
    SchemaBundle = SchemaBundle


class diagnostics:
    SpecificationError = SpecificationError


# ---------------------------------------------------------------------------
# Legacy module aliases: the validator's unit tests import a few old-engine
# module paths directly (e.g. `from yamaa.schema.row_catalog import
# expand_row_catalogs`). The clean-room package has no yamaa.schema tree,
# so expose the compatibility namespaces under those names.
# ---------------------------------------------------------------------------
import types as _types

_schema_module = _types.ModuleType("yamaa.schema")
_schema_module.__path__ = []
_row_catalog_module = _types.ModuleType("yamaa.schema.row_catalog")
_row_catalog_module.expand_row_catalogs = expand_row_catalogs
_windows_module = _types.ModuleType("yamaa.schema.windows")
_windows_module.expand_named_windows = expand_named_windows
_schema_module.row_catalog = _row_catalog_module
_schema_module.windows = _windows_module
_sys.modules["yamaa.schema"] = _schema_module
_sys.modules["yamaa.schema.row_catalog"] = _row_catalog_module
_sys.modules["yamaa.schema.windows"] = _windows_module
