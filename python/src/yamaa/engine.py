"""Clean-room derivation engine: spec loading, row construction, column
derivation, verification, and rendering."""

import copy
import os
from functools import cmp_to_key

import yaml

from . import agg as _agg
from . import expr as _expr
from . import odm as _odm
from . import pred as _pred
from . import validate as _validate
from .csv_io import parquet_field_types, read_csv, read_parquet, write_csv_text
from .errors import YamaaError
from .values import (
    INT64_MAX,
    INT64_MIN,
    YDate,
    YDateTime,
    comparable,
    compare,
    date_text,
    datetime_text,
    float_text,
    is_missing,
    normalize_number,
    parse_date,
    parse_datetime,
    parse_float_text,
    parse_int_text,
)

WINDOW_KEYS = {
    "row_number",
    "rank",
    "row_value",
    "previous_non_missing",
    "locf",
    "baseline_flag",
}


def _fail(where, phase, condition, requirement=None, context=None):
    raise YamaaError(
        phase=phase,
        condition=condition,
        requirement=requirement,
        spec_paths=[where],
        context=context or {},
    )


# The phases in which a failure belongs to one row (or partition).
_ROW_PHASES = frozenset(
    {"derivation", "impute", "join", "mapping", "cut", "row_construction", "convert"}
)


def _conversion_error(fail, v, target, path):
    """The one report a failed conversion makes: the value, its runtime type,
    and the target type."""
    return YamaaError(
        phase="convert",
        condition="conversion_failed",
        requirement=fail.requirement,
        spec_paths=[path],
        context={
            "from": _expr._runtime_type_name(v),
            "to": target,
            "value": _expr.json_value(v),
        },
    )


def _odm_no_scope(where, payload):
    """REQ-1277: an `odm` read evaluated where no row has an ODM scope."""
    read = _odm.parse_odm_read(payload)
    _fail(
        where,
        "validation",
        "invalid_odm_context",
        "REQ-1277",
        {"dataset": read.dataset if read is not None else None},
    )


def _uses_function(node):
    """True if the spec tree contains a `function:` derivation."""
    if isinstance(node, dict):
        if "function" in node:
            return True
        return any(_uses_function(v) for v in node.values())
    if isinstance(node, list):
        return any(_uses_function(v) for v in node)
    return False


class Table:
    def __init__(self, name, path, types, fields, records):
        self.name = name
        self.path = path
        self.types = types
        self.fields = fields
        self.records = records


# The requirement each failed check reports; REQ-0406 covers the rest.
_VERIFICATION_REQUIREMENTS = {
    "not_missing_failed": "REQ-0375",
    "allowed_values_failed": "REQ-0376",
    "range_failed": "REQ-0377",
    "length_failed": "REQ-0378",
    "matches_failed": "REQ-0379",
    "unique_failed": "REQ-0381",
    "all_or_none_failed": "REQ-0382",
    "implication_failed": "REQ-0383",
    "assert_failed": "REQ-0384",
    "row_count_failed": "REQ-0385",
}


# REQ-0373: an error report's sample of offending keys.
_REPORTED_KEYS = 5


def _vlist(v):
    """A verifications field is one verification mapping or a list of them."""
    if v is None:
        return []
    if isinstance(v, dict):
        return [v]
    return v


class Engine:
    def __init__(self, spec_path, project_root=None):
        self.spec_path = os.path.abspath(spec_path)
        self.spec_dir = os.path.dirname(self.spec_path)
        self.project_root = project_root
        with open(self.spec_path, "r", encoding="utf-8") as f:
            self.spec = yaml.safe_load(f)
        s = self.spec
        if not isinstance(s, dict):
            _fail("root", "validation", "invalid_field_type", "REQ-0622", {})
        self._record_row_paths()
        self._expand_named_windows()
        if s.get("schema_version") != "1.0":
            _fail(
                "schema_version",
                "validation",
                "schema_version_mismatch",
                "REQ-0656",
                {},
            )
        if s.get("parents"):
            self._reject_parents(s["parents"])
        self.functions = None
        if project_root is not None and _uses_function(s):
            from . import functions as _functions

            self.functions = _functions.load_environment(project_root)
        for req in ("domain", "keys", "input", "output", "columns"):
            if req not in s:
                _fail(req, "validation", "missing_required_field", None, {"field": req})
        self.domain = s["domain"]
        self.keys = list(s["keys"])
        self.odm_inputs = _odm.odm_inputs(s)
        self.inputs = {}
        for name, decl in s["input"].items():
            self.inputs[name] = self._load_input(name, decl)
        self.lookups_decl = {d["id"]: d for d in s.get("intermediates", []) or []}
        self.lookup_paths = {
            d["id"]: f"intermediates[{i}]"
            for i, d in enumerate(s.get("intermediates", []) or [])
        }
        # REQ-1262: named intermediates that drive a row template, as tables.
        self.drivers = {}
        self.colspecs = {}
        self.col_order = []
        for c in s["columns"]:
            if c["name"] in self.colspecs:
                _fail(
                    f"columns.{c['name']}",
                    "validation",
                    "duplicate_identifier",
                    "REQ-0227",
                    {},
                )
            self.colspecs[c["name"]] = c
            self.col_order.append(c["name"])
        self.templates = s.get("rows") or []
        self.output = s["output"]
        self.verifications = s.get("verifications") or []
        if isinstance(self.verifications, dict):
            self.verifications = [self.verifications]
        _validate.run(self)  # Stage-1 shape validation before any execution
        self.rows = []  # list[dict] completed columns
        self._origins = []  # list[str|None]
        self._recs = []  # list[dict[str, list[record]]]
        self._derived = []  # list[set[str]] columns derived per row
        self._built = []  # per row: (how it was built, its row template)
        self._lookups = {}  # (lid, row_idx) -> record | None
        self._eligible = {}  # lid -> eligible records (computed once)
        self._dict_cache = {}  # written mapping.dict path -> loaded dict
        self._join_indexes = {}  # (dataset, keys) -> implicit-join index
        self._odm_indexes = {}  # (dataset, fields) -> ODM scope index
        self._self_marks = []  # row counts after each completed template
        self._row_phase = True  # False once column derivation starts

    def _load_dict_yaml(self, written, where):
        """REQ-1110: read a mapping dictionary from a YAML project resource."""
        if written in self._dict_cache:
            return self._dict_cache[written]
        full = (
            written
            if os.path.isabs(written)
            else os.path.normpath(os.path.join(self.spec_dir, written))
        )
        if not os.path.isfile(full):
            _fail(
                where,
                "validation",
                "resource_path_missing",
                "REQ-0790",
                {"path": written},
            )
        with open(full, "r", encoding="utf-8") as f:
            data = yaml.safe_load(f)
        if (
            not isinstance(data, dict)
            or any(not isinstance(k, str) for k in data)
            or any(isinstance(v, (dict, list)) for v in data.values())
        ):
            _fail(
                where,
                "validation",
                "invalid_field_type",
                "REQ-1110",
                {"expected": "dict[str, literal_value]", "path": written},
            )
        self._dict_cache[written] = data
        return data

    def _record_row_paths(self):
        """A path addresses the specification as written: each row template
        is `rows[i]`. (Row catalogs, REQ-1249, are retired without
        replacement; validation rejects a `catalog` like any undeclared
        row field.)"""
        self._row_paths = {
            id(t): f"rows[{i}]" for i, t in enumerate(self.spec.get("rows") or [])
        }

    def row_path(self, t):
        """The written path of row template `t`: `rows[i]`."""
        return self._row_paths[id(t)]

    def deriv_path(self, t, name):
        """Where derivation `name` of template `t` is written: in the
        template, or as the column-level default the template inherits
        (REQ-1260)."""
        if name in (t.get("derivations") or {}):
            return f"{self.row_path(t)}.derivations.{name}"
        return f"columns.{name}.derivation"

    def _expand_named_windows(self):
        """REQ-1251/1252/1253: expand named window references before any"""
        s = self.spec
        windows = s.get("windows")
        defs = {}
        if windows is not None:
            if not isinstance(windows, dict):
                _fail(
                    "windows",
                    "validation",
                    "invalid_field_type",
                    "REQ-1251",
                    {
                        "expected": "dict[identifier, window_spec]",
                        "actual": type(windows).__name__,
                    },
                )
            for name, w in windows.items():
                _validate.check_named_window_definition(name, w)
                defs[name] = w

        def expand(node, where):
            if isinstance(node, dict):
                for k, v in node.items():
                    if k == "window" and isinstance(v, str):
                        if v not in defs:
                            _fail(
                                f"{where}.window",
                                "validation",
                                "unknown_window",
                                "REQ-1253",
                                {"window": v},
                            )
                        node[k] = copy.deepcopy(defs[v])
                    else:
                        expand(v, f"{where}.{k}" if where else str(k))
            elif isinstance(node, list):
                for i, v in enumerate(node):
                    expand(v, f"{where}[{i}]")

        for i, c in enumerate(s.get("columns") or []):
            if isinstance(c, dict):
                expand(c.get("derivation"), f"columns[{i}].derivation")
        for t in s.get("rows") or []:
            if isinstance(t, dict):
                for dn, dd in (t.get("derivations") or {}).items():
                    expand(dd, f"{self.row_path(t)}.derivations.{dn}")
        for i, im in enumerate(s.get("intermediates") or []):
            if isinstance(im, dict):
                for dn, dd in (im.get("derivations") or {}).items():
                    expand(dd, f"intermediates[{i}].derivations.{dn}")

    def _reject_parents(self, parents):
        """Composition failure surface (specification/composition); the
        clean-room resolves no parents."""
        if isinstance(parents, str):
            parents = [parents]
        for p in parents:
            # REQ-0653: a URL, URI, empty, or non-local parent reference.
            if not isinstance(p, str) or not p:
                reason = "empty_path"
            elif "://" in p or p.startswith("file:"):
                reason = "remote_reference"
            elif os.path.isabs(p):
                reason = "absolute_path"
            else:
                continue
            _fail(
                "parents",
                "validation",
                "invalid_parent_path",
                "REQ-0653",
                {"reason": reason},
            )
        seen = [os.path.normpath(self.spec_path)]
        for p in parents:
            self._check_parent_chain(
                os.path.normpath(os.path.join(self.spec_dir, p)),
                seen,
                self.spec.get("schema_version"),
            )
        if "output" not in self.spec:
            # REQ-0657: an entry omitting `output` inherits it; only a
            # resolution to which no layer contributed one fails.
            inherited = False
            for p in parents:
                full = os.path.normpath(os.path.join(self.spec_dir, p))
                with open(full, "r", encoding="utf-8") as f:
                    doc = yaml.safe_load(f) or {}
                if isinstance(doc, dict) and doc.get("output"):
                    inherited = True
                    break
            if not inherited:
                _fail("output", "validation", "missing_required_field", None, {})
        for c in self.spec.get("columns") or []:
            if isinstance(c, dict) and "type" in c and c["type"] is None:
                _fail(
                    f"columns.{c.get('name')}.type",
                    "validation",
                    "invalid_clear",
                    "REQ-0660",
                    {"field": "type"},
                )
        _fail(
            "parents",
            "validation",
            "invalid_clear",
            "REQ-0632",
            {"detail": "spec composition is not implemented in the clean-room"},
        )

    def _check_parent_chain(self, full, seen, entry_version):
        if full in seen:
            _fail(
                "parents",
                "validation",
                "inheritance_cycle",
                "REQ-0655",
                {"reason": "parent_chain_returns_to_entry"},
            )
        seen.append(full)
        with open(full, "r", encoding="utf-8") as f:
            doc = yaml.safe_load(f) or {}
        version = doc.get("schema_version") if isinstance(doc, dict) else None
        if version != "1.0" or version != entry_version:
            # REQ-0656: report both implicated values, never a host path.
            _fail(
                "parents",
                "validation",
                "schema_version_mismatch",
                "REQ-0656",
                {"entry_version": entry_version, "parent_version": version},
            )
        sub = doc.get("parents")
        if sub:
            if isinstance(sub, str):
                sub = [sub]
            for p in sub:
                self._check_parent_chain(
                    os.path.normpath(os.path.join(os.path.dirname(full), p)),
                    seen,
                    entry_version,
                )
        seen.pop()

    def _input_profile(self, name, path):
        """The storage profile a written input path selects (REQ-0852)."""
        lower = path.lower()
        if lower.endswith(".csv"):
            return "csv"
        if lower.endswith(".parquet"):
            return "parquet"
        _fail(
            f"input.{name}.path",
            "validation",
            "source_profile_unknown",
            "REQ-0852",
            {"dataset": name, "path": path},
        )

    def _load_input(self, name, decl):
        if name in self.odm_inputs:
            return self._load_odm_input(name, decl)
        if isinstance(decl, str):
            path, types = decl, {}
        else:
            path, types = decl["path"], decl.get("types") or {}
        _validate.check_resource_path(self, name, path)  # storage/resources
        profile = self._input_profile(name, path)
        full = os.path.normpath(os.path.join(self.spec_dir, path))
        if profile == "parquet":
            # REQ-0517: the Parquet schema is the field's type authority.
            fields, records, stored = read_parquet(
                full, types, spec_path=f"input.{name}", dataset=name, written_path=path
            )
        else:
            # REQ-0517/REQ-0518: a delimited file carries no types; an
            # undeclared field is `str`, never inferred from its values.
            fields, records = read_csv(
                full, types, spec_path=f"input.{name}", dataset=name, written_path=path
            )
            stored = {f: "str" for f in fields}
        for f in types:
            if f not in fields:
                _fail(
                    f"input.{name}.types.{f}",
                    "validation",
                    "unknown_field",
                    "REQ-0532",
                    {"dataset": name, "field": f},
                )
        return Table(name, full, {**stored, **types}, fields, records)

    def _load_odm_input(self, name, decl):
        """REQ-1266..REQ-1268: read an ODM input under its fixed schema.

        The stored fields bind to the schema at validation, from the header
        or the Parquet schema; only the bound fields are read, as text under
        the schema's names, so a vendor field is neither typed nor exposed.
        The records are verified at ingest.
        """
        if isinstance(decl, dict):
            declared = next((k for k in ("types", "schema") if k in decl), None)
            if declared is not None:
                _fail(
                    f"input.{name}.{declared}",
                    "validation",
                    "odm_schema_field_type",
                    "REQ-1275",
                    {"dataset": name, "declared": declared},
                )
            path = decl["path"]
        else:
            path = decl
        _validate.check_resource_path(self, name, path)
        profile = self._input_profile(name, path)
        full = os.path.normpath(os.path.join(self.spec_dir, path))
        where = f"input.{name}.path"
        read = {"spec_path": f"input.{name}", "dataset": name, "written_path": path}
        if profile == "parquet":
            kinds = dict(parquet_field_types(full, **read))
            names = list(kinds)
        else:
            names, raw = read_csv(full, {}, **read)
        binding = _odm.bind_fields(names)
        if binding.missing:
            _fail(
                where,
                "validation",
                "odm_schema_field_missing",
                "REQ-1275",
                {"dataset": name, "fields": list(binding.missing)},
            )
        for field, found in binding.ambiguous.items():
            _fail(
                where,
                "validation",
                "odm_schema_field_ambiguous",
                "REQ-1275",
                {"dataset": name, "field": field, "stored": list(found)},
            )
        bound = [binding.stored[f] for f in _odm.ODM_SCHEMA_FIELDS]
        if profile == "parquet":
            for field, stored in zip(_odm.ODM_SCHEMA_FIELDS, bound):
                if kinds[stored] != "str":
                    _fail(
                        where,
                        "validation",
                        "odm_schema_field_type",
                        "REQ-1275",
                        {
                            "dataset": name,
                            "field": field,
                            "stored_field": stored,
                            "stored_type": kinds[stored] or "unsupported",
                        },
                    )
            _, raw, _ = read_parquet(full, {}, select=bound, **read)
            first = 1
        else:
            first = 2  # CSV records are numbered from the header, record one
        records = [
            dict(zip(_odm.ODM_SCHEMA_FIELDS, (r[b] for b in bound))) for r in raw
        ]
        for field in _odm.ODM_REQUIRED_VALUES:
            lacking = [
                n for n, r in enumerate(records, start=first) if is_missing(r[field])
            ]
            if lacking:
                _fail(
                    where,
                    "ingest",
                    "odm_schema_value_missing",
                    "REQ-1276",
                    {
                        "dataset": name,
                        "field": field,
                        "record": lacking[0],
                        "records": len(lacking),
                    },
                )
        fields = list(_odm.ODM_SCHEMA_FIELDS)
        return Table(name, full, {f: "str" for f in fields}, fields, records)

    def run(self):
        self.rows = []
        self._origins = []
        self._recs = []
        self._derived = []
        self._built = []
        self._lookups = {}
        self._eligible = {}
        self._self_marks = []
        self._row_phase = True
        self._verify_input_intermediates()
        self._build_rows()
        self._derive_columns()
        self._verify()
        return self._render()

    def _default_dataset(self):
        if self.spec.get("base"):
            return self.spec["base"]
        if len(self.inputs) == 1:
            return next(iter(self.inputs))
        _fail("base", "validation", "missing_required_field", "REQ-0064", {})

    def _build_rows(self):
        if self.templates:
            for t in self.templates:
                self._build_template(t)
                self._self_marks.append(len(self.rows))
                self._eligible = {}
                self._lookups = {}
                self._verify_self_uniques()
        else:
            self._build_key_table()

    def _build_template(self, t):
        ds = t.get("dataset") or self._default_dataset()
        if ds in self.inputs:
            table = self.inputs[ds]
        elif ds in self.lookups_decl:
            table = self._driver_table(t, ds)
        else:
            _fail(
                f"{self.row_path(t)}.dataset",
                "validation",
                "unknown_field",
                "REQ-0103",
                {},
            )
        group_by = t.get("group_by")
        filt = t.get("filter")
        derivs = t.get("derivations") or {}
        if not group_by:
            row_defaults = getattr(self, "row_defaults", set()) or set()
            missing = [n for n in row_defaults if n not in derivs]
            if missing:
                derivs = dict(derivs)
                for n in missing:
                    derivs[n] = self.colspecs[n]["derivation"]
        if group_by:
            self._build_grouped(t, table, group_by, filt, derivs)
        else:
            self._build_record_driven(t, table, filt, derivs)

    def _driver_table(self, t, lid):
        """REQ-1262: a named intermediate drives a template with its
        source-only filtered records after its derivations, in source order,
        exposing its `columns` when declared and every field otherwise."""
        if lid not in self.drivers:
            decl = self.lookups_decl[lid]
            types = _validate.intermediate_driver_types(
                self, decl, f"{self.row_path(t)}.dataset"
            )
            recs = _BaseCtx(self, self.lookup_paths[lid])._eligible(decl)
            fields = list(types)
            records = [{f: r.get(f) for f in fields} for r in recs]
            self.drivers[lid] = Table(lid, None, types, fields, records)
        return self.drivers[lid]

    def _rec_pred(self, text, record, ds, where):
        node, _ = _pred.parse(text, where)

        def resolve(name):
            parts = name.split(".")
            if len(parts) == 2 and parts[0] == ds:
                if parts[1] not in record:
                    _fail(
                        where,
                        "validation",
                        "unknown_field",
                        "REQ-0103",
                        {"identifier": name},
                    )
                return record[parts[1]]
            if len(parts) == 1 and parts[0] in record:
                return record[parts[0]]
            _fail(
                where, "validation", "unknown_field", "REQ-0189", {"identifier": name}
            )

        return _pred.evaluate(node, resolve, where)

    def _template_phases(self, t, derivs):
        """Split a template's derivations into phase A (neither a window nor
        reading one) and the stages after it (REQ-0326). A stage evaluates
        the windows whose inputs are complete over the template's rows, then,
        row by row, the scalars those windows complete; so a window may read
        a completed window column, directly or through scalar derivations."""
        where = f"{self.row_path(t)}.derivations"
        order = _validate._topo_order(derivs, _validate._unqualified_refs, where)
        nodes = {n: _norm_derivation(derivs[n], self.deriv_path(t, n)) for n in order}
        winset = {
            n
            for n in order
            if isinstance(nodes[n], dict)
            and len(nodes[n]) == 1
            and next(iter(nodes[n])) in WINDOW_KEYS
        }
        deps = {n: _validate._unqualified_refs(nodes[n]) & set(nodes) for n in order}
        closure = set(winset)
        changed = True
        while changed:
            changed = False
            for n in order:
                if n not in closure and deps[n] & closure:
                    closure.add(n)
                    changed = True
        phase_a = [n for n in order if n not in closure]
        done = set(phase_a)
        pending = [n for n in order if n in closure]
        stages = []
        while pending:
            wins = [n for n in pending if n in winset and deps[n] <= done]
            done |= set(wins)
            scalars = []
            for n in pending:
                if n not in done and n not in winset and deps[n] <= done:
                    scalars.append(n)
                    done.add(n)
            pending = [n for n in pending if n not in done]
            stages.append((wins, scalars))
        return nodes, phase_a, stages

    def _row_filter(self, t, filt, ctx):
        """REQ-0036/REQ-0068: an ungrouped filter reads the driver record,
        the candidate's derived columns, and lookup state."""
        where = f"{self.row_path(t)}.filter"
        node, _ = _pred.parse(filt, where)
        ctx.where = where

        def resolve(name):
            head, dot, field = name.partition(".")
            if dot and head == ctx.ds:
                if field not in ctx.record:
                    _fail(
                        where,
                        "validation",
                        "unknown_field",
                        "REQ-0103",
                        {"identifier": name},
                    )
                return ctx.record[field]
            if dot and head in self.lookups_decl:
                return ctx.value(name)
            if not dot and name in ctx.row:
                return ctx.row[name]
            _fail(
                where, "validation", "unknown_field", "REQ-0068", {"identifier": name}
            )

        return _pred.evaluate(node, resolve, where)

    def _lookup_match_names(self, decl):
        """Bare current-row names a lookup's match reads (REQ-0115/REQ-0121)."""
        key = decl.get("key")
        if key is None:
            vals = list(self.keys)
        elif isinstance(key, str):
            vals = [key]
        elif isinstance(key, dict):
            vals = list(key.values())
        else:
            vals = list(key)
        between = decl.get("between") or {}
        if isinstance(between.get("value"), str):
            vals.append(between["value"])
        out = set()
        for v in vals:
            if isinstance(v, str):
                if "." not in v:
                    out.add(v)
            else:
                out |= _validate._ident_refs(v)[0]
        return out

    def _filter_reads(self, t, filt, names, nodes):
        """The template derivations an ungrouped filter needs: the names it
        reads, the match values of the lookups it reads, and their
        dependencies among `names`."""
        _, idents = _pred.parse(filt, f"{self.row_path(t)}.filter")
        driver = t.get("dataset") or self._default_dataset()
        need = set()
        for ident in idents:
            head, dot, _ = ident.partition(".")
            if not dot:
                need.add(ident)
            elif head in self.lookups_decl and head != driver:
                need |= self._lookup_match_names(self.lookups_decl[head])
        todo = [n for n in need if n in nodes]
        while todo:
            n = todo.pop()
            refs = set(_validate._unqualified_refs(nodes[n]))
            for q in _validate._ident_refs(nodes[n])[1]:
                if q.split(".")[0] in self.lookups_decl:
                    refs |= self._lookup_match_names(self.lookups_decl[q.split(".")[0]])
            for r in refs:
                if r in nodes and r not in need:
                    need.add(r)
                    todo.append(r)
        return {n for n in names if n in need}

    def _eval_row_deriv(self, t, name, node, ctx):
        ctx.where = self.deriv_path(t, name)
        try:
            v = _expr.eval_expr(node, ctx)
            cs = self.colspecs.get(name)
            ctx.row[name] = v if cs is None else self._convert(v, cs, node, ctx.where)
        except YamaaError as err:
            raise self._with_keys(err, ctx.row)

    def _row_stages(self, t, ds, nodes, stages, rows, recs, ctxs):
        """Run a template's window stages over its constructed rows."""
        wctx = _RowWinCtx(self, t, ds, rows, recs)
        for wins, scalars in stages:
            for name in wins:
                kind = next(iter(nodes[name]))
                where = self.deriv_path(t, name)
                wctx.where = f"{where}.{kind}"
                vec = wctx.window_value(kind, nodes[name][kind])
                cs = self.colspecs.get(name)
                for row, v in zip(rows, vec):
                    try:
                        row[name] = (
                            v
                            if cs is None
                            else self._convert(v, cs, nodes[name], where)
                        )
                    except YamaaError as err:
                        raise self._with_keys(err, row)
            for ctx in ctxs:
                for name in scalars:
                    self._eval_row_deriv(t, name, nodes[name], ctx)

    def _build_record_driven(self, t, table, filt, derivs):
        """REQ-0036: every driver record builds a candidate row; the filter
        gates it once the derivations it reads are complete, before the
        window pass. A discarded record derives nothing else."""
        ds = table.name
        nodes, phase_a, stages = self._template_phases(t, derivs)
        gate = set()
        if filt is not None:
            gate = self._filter_reads(t, filt, phase_a, nodes)
        first = [n for n in phase_a if n in gate]
        rest = [n for n in phase_a if n not in gate]
        rows, ctxs, recs = [], [], []
        for rec in table.records:
            ctx = _RowCtx(self, "row", t, ds, record=rec)
            ctx.row = {}
            for name in first:
                self._eval_row_deriv(t, name, nodes[name], ctx)
            if filt is not None and self._row_filter(t, filt, ctx) is not True:
                continue
            for name in rest:
                self._eval_row_deriv(t, name, nodes[name], ctx)
            rows.append(ctx.row)
            ctxs.append(ctx)
            recs.append(rec)
        self._row_stages(t, ds, nodes, stages, rows, recs, ctxs)
        for row, rec in zip(rows, recs):
            self._append_row(row, ds, {ds: [rec]}, set(derivs), ("record", t))

    def _build_grouped(self, t, table, group_by, filt, derivs):
        ds = table.name
        gcols = [g.split(".")[-1] for g in group_by]
        for g in group_by:
            if "." not in g or g.split(".")[0] != ds:
                _fail(
                    f"{self.row_path(t)}.group_by",
                    "validation",
                    "unknown_field",
                    "REQ-0066",
                    {"group_by": g},
                )
        groups = []  # (key_tuple, key_dict, records)
        index = {}
        for rec in table.records:
            key = tuple(_hashable(rec.get(c)) for c in gcols)
            if key not in index:
                index[key] = len(groups)
                groups.append([key, {c: rec.get(c) for c in gcols}, []])
            groups[index[key]][2].append(rec)
        nodes, phase_a, stages = self._template_phases(t, derivs)
        where = f"{self.row_path(t)}.filter"
        rows, ctxs, grecs = [], [], []
        for _, keydict, grecords in groups:
            ctx = _RowCtx(self, "row", t, ds, group=grecords, groupkeys=keydict)
            ctx.row = {}
            for name in phase_a:
                self._eval_row_deriv(t, name, nodes[name], ctx)
            if filt is not None:
                node, _ = _pred.parse(filt, where)
                row = ctx.row
                if (
                    _pred.evaluate(node, lambda n, row=row: row.get(n), where)
                    is not True
                ):
                    continue
            rows.append(ctx.row)
            ctxs.append(ctx)
            grecs.append(grecords)
        self._row_stages(t, ds, nodes, stages, rows, None, ctxs)
        for row, grecords in zip(rows, grecs):
            self._append_row(row, ds, {ds: grecords}, set(derivs), ("group", t))

    def _build_key_table(self):
        base = self._default_dataset()
        table = self.inputs[base]
        records = table.records
        filt = self.spec.get("filter")
        if filt is not None:
            if not isinstance(filt, str):
                _fail(
                    "filter",
                    "validation",
                    "invalid_field_type",
                    "REQ-1042",
                    {"expected": "predicate", "actual": type(filt).__name__},
                )
            records = [
                rec
                for rec in records
                if self._rec_pred(filt, rec, base, "filter") is True
            ]
        keyderivs = {}
        for k in self.keys:
            cs = self.colspecs.get(k)
            if cs is None:
                _fail("keys", "validation", "unknown_field", "REQ-0220", {"key": k})
            d = cs.get("derivation")
            if d is None:
                _fail(
                    f"columns.{k}",
                    "validation",
                    "missing_required_field",
                    "REQ-0042",
                    {"key": k},
                )
            keyderivs[k] = d
        keynodes = {
            k: _norm_derivation(d, f"columns.{k}.derivation")
            for k, d in keyderivs.items()
        }
        winkeys = [k for k in self.keys if next(iter(keynodes[k])) in WINDOW_KEYS]
        plainkeys = [k for k in self.keys if k not in winkeys]
        krows, krecs = [], []
        for rec in records:
            ctx = _KeyCtx(self, base, rec)
            row = {}
            for k in plainkeys:
                ctx.where = f"columns.{k}.derivation"
                v = _expr.eval_expr(keynodes[k], ctx)
                row[k] = self._convert(v, self.colspecs[k])
            krows.append(row)
            krecs.append(rec)
        for k in winkeys:
            node = keynodes[k]
            kind = next(iter(node))
            where = f"columns.{k}.derivation.{kind}"
            wctx = _KeyWinCtx(self, base, krows, krecs, where)
            vec = wctx.window_value(kind, node[kind])
            for i, v in enumerate(vec):
                krows[i][k] = self._convert(v, self.colspecs[k])
        seen = {}
        order = []
        for i, row in enumerate(krows):
            tup = tuple(_hashable(row[k]) for k in self.keys)
            if tup not in seen:
                seen[tup] = len(order)
                order.append(row)
                self._origins.append(base)
                self._recs.append({base: []})
                self._derived.append(set(self.keys))
                self._built.append(("key", None))
            self._recs[seen[tup]][base].append(krecs[i])
        self.rows = order

    def _join_records(self, table, keys, match_vals):
        """REQ-0133/REQ-0136: the implicit join's records for one current
        row, through an equality index built once per dataset and key list
        instead of a scan per read (issue #1485)."""
        probe = _join_key(match_vals)
        if probe is None:
            return []
        ck = (table.name, tuple(keys))
        index = self._join_indexes.get(ck)
        if index is None:
            index = {}
            for r in table.records:
                tup = _join_key([r.get(k) for k in keys])
                if tup is not None:
                    index.setdefault(tup, []).append(r)
            self._join_indexes[ck] = index
        return list(index.get(probe, ()))

    def _append_row(self, row, origin, recs, derived, built):
        self.rows.append(row)
        self._origins.append(origin)
        self._recs.append(recs)
        self._derived.append(set(derived))
        self._built.append(built)

    def _odm_scope(self, ds, built, recs, row, item_oid):
        """REQ-1269: the records of a row's ODM scope that carry an item,
        and the values that scope is taken on.

        A key combination reads the records it was derived from, a grouped
        row the records equal to it on the hierarchy fields of its template's
        `group_by`, and a record-driven row the records equal to its driver
        record on all eight. Missing equals missing, as REQ-0037 groups.
        """
        kind, t = built
        if kind == "key":
            scope = {k: row.get(k) for k in self.keys}
            return [r for r in recs if r.get("ItemOID") == item_oid], scope
        if kind == "group":
            group_by = set(t.get("group_by") or ())
            fields = tuple(
                f for f in _odm.ODM_HIERARCHY_FIELDS if f"{ds}.{f}" in group_by
            )
        else:
            fields = _odm.ODM_HIERARCHY_FIELDS
        ck = (ds, fields)
        index = self._odm_indexes.get(ck)
        if index is None:
            index = {}
            for r in self.inputs[ds].records:
                tup = tuple(_hashable(r.get(f)) for f in (*fields, "ItemOID"))
                index.setdefault(tup, []).append(r)
            self._odm_indexes[ck] = index
        anchor = recs[0]
        probe = tuple(_hashable(anchor.get(f)) for f in fields)
        scope = {f: anchor.get(f) for f in fields}
        return list(index.get((*probe, _hashable(item_oid)), ())), scope

    def _odm_read(self, payload, ds_of_row, built, recs, row, where, phase):
        """REQ-1271/REQ-1272: read the one record an `odm` expression
        identifies in the row's scope. None identified gives missing, one its
        Value, and two or more fail even when their values agree."""
        read = _odm.parse_odm_read(payload)
        if read is None or read.dataset != ds_of_row or not recs:
            _fail(
                where,
                "validation",
                "invalid_odm_context",
                "REQ-1277",
                {"dataset": getattr(read, "dataset", None)},
            )
        ds = read.dataset
        found, scope = self._odm_scope(ds, built, recs, row, read.item_oid)
        for level, wanted in (
            ("StudyEventOID", read.events),
            ("FormOID", read.forms),
            ("ItemGroupOID", read.item_groups),
        ):
            if wanted is not None:
                found = [r for r in found if r.get(level) in wanted]
        if read.filter is not None:
            node, _ = _pred.parse(read.filter, f"{where}.filter")

            def resolve(name, _r=None):
                head, _, field = name.partition(".")
                if head != ds or field not in _odm.ODM_SCHEMA_FIELDS:
                    _fail(
                        f"{where}.filter",
                        "validation",
                        "unknown_field",
                        "REQ-1271",
                        {"identifier": name, "dataset": ds},
                    )
                return _r.get(field)

            found = [
                r
                for r in found
                if _pred.evaluate(
                    node, lambda n, _r=r: resolve(n, _r), f"{where}.filter"
                )
                is True
            ]
        if not found:
            return None
        if len(found) == 1:
            return found[0].get("Value")
        differ = {}
        for field in _odm.ODM_IDENTIFYING_FIELDS:
            seen = []
            for r in found:
                if r.get(field) not in seen:
                    seen.append(r.get(field))
            if len(seen) > 1:
                differ[field] = seen
        _fail(
            where,
            phase,
            "odm_not_unique",
            "REQ-1278",
            {
                "item": read.item,
                "row": (built[1] or {}).get("id"),
                "scope": scope,
                "records": len(found),
                "differ": differ,
                "repeated": not differ,
            },
        )

    def _derive_columns(self):
        self._row_phase = False
        for name in self.col_order:
            cs = self.colspecs[name]
            deriv = cs.get("derivation")
            if deriv is None:
                for i in range(len(self.rows)):
                    if name not in self._derived[i]:
                        self.rows[i][name] = None
                        self._derived[i].add(name)
                continue
            node = _norm_derivation(deriv, f"columns.{name}.derivation")
            key = next(iter(node))
            if key in WINDOW_KEYS:
                self._derive_window(name, cs, node, key)
                continue
            for i in range(len(self.rows)):
                if name in self._derived[i]:
                    continue
                ctx = _ColCtx(self, i, f"columns.{name}.derivation")
                try:
                    v = _expr.eval_expr(node, ctx)
                    self.rows[i][name] = self._convert(v, cs)
                except YamaaError as err:
                    raise self._with_keys(err, self.rows[i])
                self._derived[i].add(name)
        self._verify_self_uniques()

    def _derive_window(self, name, cs, node, kind):
        payload = node[kind]
        ctx = _ColCtx(self, 0, f"columns.{name}.derivation.{kind}")
        vec = ctx.window_value(kind, payload)
        for i, v in enumerate(vec):
            if name in self._derived[i]:
                continue
            try:
                self.rows[i][name] = self._convert(v, cs)
            except YamaaError as err:
                raise self._with_keys(err, self.rows[i])
            self._derived[i].add(name)

    def _convert(self, v, cs, node=None, where=None, path=None):
        """REQ-0010: convert a completed result to its column's type. The
        failure cites the conversion rule that refused the value (REQ-0013,
        REQ-0021, REQ-0601) and is the column's own, at `columns.<name>`
        (or `path`), unless `unconvertible` answers it (REQ-0359). A handler
        literal that does not convert fails at the handler, under the
        derivation written at `where` (REQ-0364)."""
        target = cs["type"]
        node = node if node is not None else cs.get("derivation")
        try:
            return _convert_value(v, target)
        except _ConvertFail as fail:
            if not (isinstance(node, dict) and "unconvertible" in node):
                raise _conversion_error(
                    fail, v, target, path or f"columns.{cs['name']}"
                )
        handler = node["unconvertible"]
        try:
            return _convert_value(handler, target)
        except _ConvertFail as fail:
            where = where or f"columns.{cs['name']}.derivation"
            raise _conversion_error(fail, handler, target, f"{where}.unconvertible")

    def _with_keys(self, err, row):
        """A failure evaluating one row names the row by its output keys once
        every key is derived; a partial key names no row."""
        if err.phase in _ROW_PHASES and "keys" not in err.context:
            keys = _expr.keys_of(self, row)
            if keys is not None:
                err.context = {**err.context, "keys": [keys]}
        return err

    def _verify(self):
        """REQ-0031: column verification completes, then the output keys
        are validated, then dataset verification runs."""
        for name in self.col_order:
            for i, v in enumerate(_vlist(self.colspecs[name].get("verifications"))):
                where = f"columns.{name}.verifications[{i}]"
                self._verify_one(v, where, name)
        self._check_keys()
        for i, v in enumerate(self.verifications):
            self._verify_one(v, f"verifications[{i}]", None)

    def _check_keys(self):
        """REQ-0240: every key is present on every row and no two rows share
        a combination. A missing key fails at its `keys[i]` entry and a
        repeated combination at `keys`, each reporting its count and a
        sample of the offending keys."""
        for pos, k in enumerate(self.keys):
            offending = [i for i, r in enumerate(self.rows) if is_missing(r.get(k))]
            if offending:
                _, ctx = self._offending(
                    "missing_key", offending, count_name="missing_count", column=k
                )
                _fail(f"keys[{pos}]", "output", "missing_key", "REQ-0240", ctx)
        parts = {}
        for i, row in enumerate(self.rows):
            tup = tuple(_hashable(row.get(k)) for k in self.keys)
            parts.setdefault(tup, []).append(i)
        repeated = [p[0] for p in parts.values() if len(p) > 1]
        if repeated:
            _, ctx = self._offending(
                "duplicate_key", repeated, count_name="duplicate_count"
            )
            _fail("keys", "output", "duplicate_key", "REQ-0240", ctx)

    def _verify_self_uniques(self):
        """REQ-0120/1245: uniqueness over the current completed donor pool."""
        pool = (
            self.rows[: self._self_marks[-1]]
            if self._row_phase and self._self_marks
            else self.rows
        )
        for decl in self.lookups_decl.values():
            if decl.get("dataset") == "SELF" and decl.get("verifications"):
                self._check_intermediate_uniques(decl, pool)

    def _verify_input_intermediates(self):
        """REQ-1245: an input-backed check runs before any row is built,
        whether or not a row reads the intermediate, so a filter or
        derivation that fails to materialize fails here as well."""
        ctx = _BaseCtx(self, "<intermediates>")
        for decl in self.lookups_decl.values():
            if decl.get("dataset") != "SELF" and decl.get("verifications"):
                ctx.where = self.lookup_paths[decl["id"]]
                ctx._eligible(decl)

    def _check_intermediate_uniques(self, decl, recs):
        """REQ-1245: each `unique` check holds over the filtered donor
        records; a repeated combination fails at the check's own path."""
        where = self.lookup_paths[decl["id"]]
        for path, cols in _validate.intermediate_uniques(decl, where):
            seen, repeated = set(), set()
            for r in recs:
                tup = tuple(_hashable(r.get(c)) for c in cols)
                if tup in seen:
                    repeated.add(tup)
                seen.add(tup)
            if repeated:
                _fail(
                    path,
                    "verification",
                    "duplicate_intermediate_records",
                    "REQ-1245",
                    {
                        "intermediate": decl["id"],
                        "dataset": decl["dataset"],
                        "columns": list(cols),
                        "duplicate_count": len(repeated),
                    },
                )

    def _verify_one(self, v, where, col):
        if not isinstance(v, dict) or len(v) != 1:
            _fail(where, "validation", "invalid_field_type", "REQ-0397", {})
        kind, payload = next(iter(v.items()))
        where = f"{where}.{kind}"
        payload = payload or {}
        if kind == "unique" and isinstance(payload, list):
            payload = {"columns": payload}  # REQ-0381: the unnamed form
        if not isinstance(payload, dict):
            _fail(where, "validation", "invalid_field_type", "REQ-0397", {})
        severity = payload.get("severity", "error")
        failed = self._check_verification(kind, payload, col, where)
        if failed is not None and severity == "error":
            condition, detail = failed
            report = {}
            if payload.get("id") is not None:
                report["verification_id"] = payload["id"]  # REQ-0374
            _fail(
                where,
                "verification",
                condition,
                _VERIFICATION_REQUIREMENTS.get(condition, "REQ-0406"),
                {**report, **detail},
            )

    def _offending(
        self, condition, rows, count=None, count_name="failure_count", **context
    ):
        """REQ-0373: a failed check reports its failure count and a
        representative sample of the offending rows' keys."""
        keys = [_expr.keys_of(self, self.rows[i]) for i in rows[:_REPORTED_KEYS]]
        count = len(rows) if count is None else count
        return condition, {**context, count_name: count, "keys": keys}

    def _check_verification(self, kind, payload, col, where):
        """The failed condition and its report, or None when the check
        holds. Rows are checked in construction order."""
        rows = range(len(self.rows))
        if kind == "row_count":
            return self._check_row_count(payload, where)
        if kind == "unique":
            cols = payload["columns"]
            parts = {}
            for i, row in enumerate(self.rows):
                tup = tuple(_hashable(row.get(c)) for c in cols)
                parts.setdefault(tup, []).append(i)
            repeated = [p for p in parts.values() if len(p) > 1]
            if not repeated:
                return None
            # Each repeated combination is one failure; every row carrying
            # it is offending, since those rows differ in their own keys.
            offending = [i for p in repeated for i in p]
            return self._offending(
                "unique_failed", offending, len(repeated), columns=list(cols)
            )
        if kind == "all_or_none":
            offending = []
            for i, row in enumerate(self.rows):
                vals = [row.get(c) for c in payload["columns"]]
                if any(is_missing(x) for x in vals) and not all(
                    is_missing(x) for x in vals
                ):
                    offending.append(i)
            return (
                self._offending("all_or_none_failed", offending) if offending else None
            )
        if kind == "assert":
            node, _ = _pred.parse(payload["expr"], f"{where}.expr")
            offending = [
                i
                for i in rows
                if _pred.evaluate(node, _ColCtx(self, i, where).value, where)
                is not True
            ]
            return self._offending("assert_failed", offending) if offending else None
        if kind == "implies":
            wn, _ = _pred.parse(payload["when"], f"{where}.when")
            tn, _ = _pred.parse(payload["then"], f"{where}.then")
            offending = []
            for i in rows:
                ctx = _ColCtx(self, i, where)
                w = _pred.evaluate(wn, ctx.value, where)
                if w is True and _pred.evaluate(tn, ctx.value, where) is not True:
                    offending.append(i)
            if not offending:
                return None
            return self._offending("implication_failed", offending)
        col = col or payload.get("column")
        if col is None or col not in self.colspecs:
            _fail(
                where, "validation", "unknown_field", "REQ-0405", {"verification": kind}
            )
        values = [(i, self.rows[i].get(col)) for i in rows]
        present = [(i, v) for i, v in values if not is_missing(v)]
        extra = {}
        if kind == "not_missing":
            condition = "not_missing_failed"
            offending = [i for i, v in values if is_missing(v)]
        elif kind == "allowed_values":
            condition = "allowed_values_failed"
            allowed = set(payload["values"])
            offending = [i for i, v in present if v not in allowed]
        elif kind == "range":
            condition = "range_failed"
            offending = [
                i
                for i, v in present
                if ("min" in payload and compare(v, payload["min"]) < 0)
                or ("max" in payload and compare(v, payload["max"]) > 0)
            ]
        elif kind == "max_length":
            condition = "length_failed"
            offending = [i for i, v in present if len(v) > payload["max"]]
            extra["max"] = payload["max"]
        elif kind == "matches":
            condition = "matches_failed"
            rx = _validate._normalize_pattern(payload["pattern"])
            offending = [i for i, v in present if not rx.search(v)]
        else:
            _fail(
                where, "validation", "unknown_field", "REQ-0397", {"verification": kind}
            )
        if not offending:
            return None
        return self._offending(condition, offending, column=col, **extra)

    def _check_row_count(self, payload, where):
        """REQ-0385/0386/0387/1154: group the artifact's rows, count the
        rows `filter` admits in each group, and bound the groups `when`
        binds. The denominator of a fraction is the unfiltered group."""

        def admits(text, i):
            node, _ = _pred.parse(text, where)
            return (
                _pred.evaluate(node, lambda n, i=i: self.rows[i].get(n), where) is True
            )

        filt, when = payload.get("filter"), payload.get("when")
        group_by = payload.get("group_by") or []
        groups = [list(range(len(self.rows)))]
        if group_by:
            parts = {}
            for i in range(len(self.rows)):
                tup = tuple(_hashable(self.rows[i].get(g)) for g in group_by)
                parts.setdefault(tup, []).append(i)
            groups = list(parts.values())
        keys, counts = [], []
        for g in groups:
            if when is not None and not any(admits(when, i) for i in g):
                continue
            n = sum(1 for i in g if filt is None or admits(filt, i))
            frac = n / len(g) if g else 0.0
            for bound, value, low in (
                ("min", n, True),
                ("max", n, False),
                ("min_fraction", frac, True),
                ("max_fraction", frac, False),
            ):
                b = payload.get(bound)
                if b is not None and (value < b if low else value > b):
                    row = self.rows[g[0]]
                    keys.append({c: _expr.json_value(row.get(c)) for c in group_by})
                    counts.append(n)
                    break
        if not keys:
            return None
        # REQ-0402: the report names each failing group and its count; the
        # bounds are in the declaration at the reported path.
        detail = {"failure_count": len(keys), "keys": keys, "counts": counts}
        return "row_count_failed", detail

    def _render(self):
        rows = self.rows
        if self.output.get("order_by"):
            rows = _order_rows(
                rows, self.output["order_by"], lambda r, n: r.get(n), "output.order_by"
            )
        cols = self.output["columns"]
        for c in cols:
            if c not in self.colspecs:
                _fail(
                    "output.columns",
                    "validation",
                    "undeclared_column",
                    "REQ-0234",
                    {"column": c},
                )
        ctypes = {c: self.colspecs[c]["type"] for c in cols}
        out_rows = [{c: r.get(c) for c in cols} for r in rows]
        return write_csv_text(
            cols, out_rows, ctypes, decimals=self.output.get("decimals")
        )


_ABSENT = _expr._ABSENT


def _join_key(values):
    """A hashable image of match values that equal exactly when REQ-0005
    compares them equal: numbers by value across int and float, strings
    exactly, and each temporal type only with itself. None when a value is
    missing or has no equality at all (a Boolean), so nothing matches."""
    out = []
    for v in values:
        if is_missing(v) or isinstance(v, bool):
            return None
        if isinstance(v, (int, float)):
            out.append(("num", float(v)))
        elif isinstance(v, str):
            out.append(("str", v))
        elif isinstance(v, YDateTime):
            out.append(
                ("datetime", (v.year, v.month, v.day, v.hour, v.minute, v.second))
            )
        elif isinstance(v, YDate):
            out.append(("date", v.toordinal()))
        else:
            return None
    return tuple(out)


def _hashable(v):
    if isinstance(v, (YDate, YDateTime)):
        return ("__h__", "date", v.isoformat(), getattr(v, "precision", "D"))
    if isinstance(v, float):
        return ("__h__", "float", v)
    if isinstance(v, (str, int, bool)) or v is None:
        return ("__h__", type(v).__name__, v)
    return ("__h__", "repr", repr(v))


def _derive_qualifiers(node, out):
    """Collect dataset qualifiers named inside a derive binding's derivation."""
    if isinstance(node, str):
        head, dot, _ = node.partition(".")
        if dot and head:
            out.add(head)
        return
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "literal":
                continue
            _derive_qualifiers(v, out)
    elif isinstance(node, list):
        for v in node:
            _derive_qualifiers(v, out)


def _derive_var_names(node, out):
    """Collect fully-qualified variable names inside a derive binding."""
    if isinstance(node, str):
        head, dot, _ = node.partition(".")
        if dot and head:
            out.add(node)
        return
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "literal":
                continue
            _derive_var_names(v, out)
    elif isinstance(node, list):
        for v in node:
            _derive_var_names(v, out)


def _norm_derivation(d, where):
    """REQ-0266/REQ-0319: bare string -> {source: s}; {value:} handled wrapper."""
    if isinstance(d, str):
        return {"source": d}
    if isinstance(d, dict) and len(d) == 1:
        return d
    if isinstance(d, dict) and "value" in d and set(d) <= _expr.HANDLED_FIELDS:
        return d
    _fail(
        where,
        "validation",
        "invalid_field_type",
        _expr.derivation_requirement(d),
        {"derivation": d},
    )


def _eval_intermediate_derivs(e, decl, recs):
    """REQ-1185: derivations in declaration order; windows compute over the
    full donor set as augmented by earlier derivations. A failing derivation
    fails at its own path."""
    ds = decl["dataset"]
    augmented = [dict(r) for r in recs]
    for dname, dnode in decl["derivations"].items():
        where = f"{e.lookup_paths[decl['id']]}.derivations.{dname}"
        node = _norm_derivation(dnode, where)
        kind = next(iter(node)) if isinstance(node, dict) and len(node) == 1 else None
        if kind in _validate._WINDOW_KINDS:
            wctx = _DonorWinCtx(e, ds, augmented, where)
            vec = wctx.window_value(kind, node[kind])
            for rr, v in zip(augmented, vec):
                rr[dname] = v
        else:
            for j, rr in enumerate(augmented):
                rr[dname] = _expr.eval_expr(node, _DonorCtx(e, decl, rr, j, where))
    return augmented


class _BaseCtx:
    """Shared expression-context protocol for _expr."""

    def __init__(self, engine, where):
        self.e = engine
        self.where = where

    def value(self, name):
        raise NotImplementedError

    def _match_operand(self, name):
        """Bare-operand resolution for match-value expressions. Row"""
        return self.value(name)

    def source_records(self, var):
        raise NotImplementedError

    def value_field(self, var):
        return var.split(".")[-1]

    def _win_val(self, e, i, var):
        """Resolve a window variable for output row i. Unqualified names read"""
        if "." in var:
            return _ColCtx(e, i, self.where).value(var)
        return e.rows[i].get(var)

    def _win_n(self):
        """Number of rows the window machinery partitions. Overridden by
        contexts whose window rows are not the output rows (key phase)."""
        return len(self.e.rows)

    def odm_value(self, payload):
        """REQ-1270/REQ-1277: only a row built from an ODM input has a
        scope; a named intermediate's record has none."""
        _odm_no_scope(self.where, payload)

    def record_predicate(self, text, record, default_ds=None):
        node, _ = _pred.parse(text, self.where)
        ds = record.get("_ds") or default_ds

        def resolve(name):
            parts = name.split(".")
            if len(parts) == 2 and parts[0] == ds:
                return record.get(parts[1])
            if len(parts) == 1:
                return record.get(name)
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0189",
                {"identifier": name},
            )

        return _pred.evaluate(node, resolve, self.where)

    def order_records(self, recs, order_by):
        return _order_records(recs, order_by, self.where)

    def _row_index(self):
        raise NotImplementedError

    def _driver_intermediate(self):
        """The named intermediate driving the current row, if any."""

    def _is_lookup(self, ds):
        """An intermediate's qualifier reads a keyed lookup, except for the
        intermediate driving the current row, which reads that row's driver
        record (REQ-1262)."""
        return ds in self.e.lookups_decl and ds != self._driver_intermediate()

    def _table(self, ds):
        """The input `ds` names, or the current row's driver intermediate."""
        if ds is not None and ds == self._driver_intermediate():
            return self.e.drivers[ds]
        return self.e.inputs.get(ds)

    def _driver_ds(self):
        """REQ-0120: the driver dataset is the root base, or the sole input
        when no base is declared."""
        base = self.e.spec.get("base")
        if base is not None:
            return base
        ins = list(self.e.inputs)
        return ins[0] if len(ins) == 1 else None

    def _correlated_record(self, driver, i):
        """The current driver record a correlated filter reads for row i."""
        if not (isinstance(i, int) and 0 <= i < len(self.e._recs)):
            return None
        ds_recs = (self.e._recs[i] or {}).get(driver, [])
        return ds_recs[0] if ds_recs else None

    def _correlated_filter(self, decl):
        """Parse a correlated intermediate filter once."""
        filt = decl.get("filter")
        if not filt or not isinstance(filt, str):
            return None
        driver = self._driver_ds()
        donor = decl["dataset"]
        if driver is None or driver == donor:
            return None
        node, idents = _pred.parse(filt, self.where)
        for ident in idents:
            parts = ident.split(".")
            if len(parts) == 2 and parts[0] == driver:
                return node, driver, donor
        return None

    def _eligible(self, decl):
        lid = decl["id"]
        if decl["dataset"] == "SELF":
            if not self.e._row_phase:
                return self._self_eligible(decl)
            if lid not in self.e._eligible:
                self.e._eligible[lid] = self._self_eligible(decl)
            return self.e._eligible[lid]
        if lid not in self.e._eligible:
            table = self.e.inputs[decl["dataset"]]
            recs = table.records
            ds = table.name
            if decl.get("derivations"):
                recs = _eval_intermediate_derivs(self.e, decl, recs)
            corr = self._correlated_filter(decl)
            if decl.get("filter") and corr is None:
                node, _ = _pred.parse(decl["filter"], self.where)

                def resolve(name, _r=None):
                    parts = name.split(".")
                    if len(parts) == 2 and parts[0] == ds:
                        return _r.get(parts[1])
                    if len(parts) == 1:
                        return _r.get(name)
                    _fail(
                        self.where,
                        "validation",
                        "unknown_field",
                        "REQ-0132",
                        {"identifier": name},
                    )

                out = []
                for r in recs:
                    if (
                        _pred.evaluate(node, lambda n, _r=r: resolve(n, _r), self.where)
                        is True
                    ):
                        rr = dict(r)
                        rr["_ds"] = ds
                        out.append(rr)
                recs = out
            else:
                recs = [dict(r, _ds=table.name) for r in recs]
            self.e._check_intermediate_uniques(decl, recs)
            self.e._eligible[lid] = recs
        return self.e._eligible[lid]

    def _self_eligible(self, decl):
        """REQ-0120/0136: SELF donors are completed rows (earlier templates
        during row construction; all rows during column derivation)."""
        mark = self.e._self_marks[-1] if self.e._self_marks else 0
        donors = (
            [dict(r) for r in self.e.rows[:mark]]
            if self.e._row_phase
            else [dict(r) for r in self.e.rows]
        )
        if decl.get("derivations"):
            donors = _eval_intermediate_derivs(self.e, decl, donors)
        corr = self._correlated_filter(decl)
        filt = decl.get("filter")
        if filt and corr is None:
            node, _ = _pred.parse(filt, self.where)
            donors = [
                r
                for r in donors
                if _pred.evaluate(
                    node, lambda n, _r=r: self._self_resolve(n, _r), self.where
                )
                is True
            ]
        return donors

    def _self_resolve(self, name, record):
        """REQ-0120: bare and SELF-qualified names read donor fields."""
        parts = name.split(".")
        if len(parts) == 2 and parts[0] == "SELF":
            return record.get(parts[1])
        if len(parts) == 1:
            return record.get(name)
        _fail(
            self.where, "validation", "unknown_field", "REQ-0132", {"identifier": name}
        )

    def _lookup_keys(self, decl):
        """Return (donor_fields, driver_exprs) for a lookup key.

        REQ-0115: key is a list of donor fields matched by same-named
        current-row values, or a mapping of donor field -> driver match
        expression. REQ-1185: a derived name is a donor field too.
        """
        where = self.e.lookup_paths[decl["id"]]
        if decl["dataset"] == "SELF":
            fields = set(getattr(self.e, "donor_fields", None) or set())
            ds_label = "SELF"
        else:
            table = self.e.inputs[decl["dataset"]]
            fields = set(table.fields)
            ds_label = decl["dataset"]
        fields |= set(decl.get("derivations") or {})
        key = decl.get("key")
        if key is None:
            key = [k for k in self.e.keys if k in fields]
            if not key:
                # REQ-0153: an omitted key is the output keys the dataset
                # carries; with none, the key must be declared.
                _fail(
                    where,
                    "validation",
                    "no_applicable_keys",
                    "REQ-0153",
                    {
                        "intermediate": decl["id"],
                        "dataset": ds_label,
                        "keys": list(self.e.keys),
                        "hint": "declare the `key` explicitly",
                    },
                )
        elif isinstance(key, str):
            key = [key]
        if isinstance(key, dict):
            key_fields = list(key.keys())
            key_exprs = list(key.values())
        else:
            key_fields = list(key)
            key_exprs = list(key)
        if not key_fields:
            _fail(
                where + ".key",
                "validation",
                "invalid_field_type",
                "REQ-0115",
                {"expected": "at least one key pair"},
            )
        for k in key_fields:
            if k not in fields:
                _fail(
                    where + ".key",
                    "validation",
                    "unknown_field",
                    "REQ-0116",
                    {"intermediate": decl["id"], "key": k},
                )
        return key_fields, key_exprs

    def _match_context(self, decl, key_fields, match_vals, i):
        """REQ-0143/REQ-0144: the match a lookup failure reports."""
        out = {
            "intermediate": decl["id"],
            "dataset": decl["dataset"],
            "key": list(key_fields),
            "intermediate_key": dict(zip(key_fields, match_vals)),
        }
        if isinstance(i, int) and 0 <= i < len(self.e.rows):
            out["keys"] = [{k: self.e.rows[i].get(k) for k in self.e.keys}]
        return out

    def _match_lookup(self, decl, i):
        cache_key = (decl["id"], i)
        if cache_key in self.e._lookups:
            return self.e._lookups[cache_key]
        recs = self._eligible(decl)
        key_fields, key_exprs = self._lookup_keys(decl)
        match_vals = [self._match_value(v, i) for v in key_exprs]
        cands = []
        if not any(is_missing(v) for v in match_vals):
            for r in recs:
                ok = True
                for k, mv in zip(key_fields, match_vals):
                    rv = r.get(k)
                    if is_missing(rv) or not comparable(mv, rv) or compare(mv, rv) != 0:
                        ok = False
                        break
                if ok:
                    cands.append(r)
            if decl.get("between"):
                cands = self._apply_between(cands, decl, i)
            corr = self._correlated_filter(decl)
            if corr is not None:
                node, driver, donor = corr
                drec = self._correlated_record(driver, i)

                def resolve(name, _r=None):
                    parts = name.split(".")
                    if donor == "SELF":
                        if len(parts) == 2 and parts[0] == "SELF":
                            return _r.get(parts[1])
                        if len(parts) == 1:
                            return _r.get(name)
                    elif len(parts) == 2 and parts[0] == donor:
                        return _r.get(parts[1])
                    if len(parts) == 2 and parts[0] == driver:
                        return drec.get(parts[1]) if drec is not None else None
                    _fail(
                        self.where,
                        "validation",
                        "unknown_field",
                        "REQ-0132",
                        {"identifier": name},
                    )

                cands = [
                    r
                    for r in cands
                    if _pred.evaluate(node, lambda n, _r=r: resolve(n, _r), self.where)
                    is True
                ]
            if len(cands) > 1:
                ob, keep = decl.get("order_by"), decl.get("keep")
                if not ob or not keep:
                    ctx = self._match_context(decl, key_fields, match_vals, i)
                    ctx["match_count"] = len(cands)
                    _fail(
                        self.e.lookup_paths[decl["id"]],
                        "join",
                        "multiple_matches",
                        "REQ-0127",
                        ctx,
                    )
                cands = _order_records(cands, ob, self.where)
                cands = [cands[0] if keep == "first" else cands[-1]]
        if not cands:
            # REQ-0124: no_match answers the read; without it, fail.
            if "no_match" in decl:
                self.e._lookups[cache_key] = None
                return None
            _fail(
                self.e.lookup_paths[decl["id"]],
                "join",
                "unmatched_key",
                "REQ-0124",
                self._match_context(decl, key_fields, match_vals, i),
            )
        self.e._lookups[cache_key] = cands[0]
        return cands[0]

    def _match_value(self, var, i):
        if isinstance(var, dict):
            return _expr.eval_expr(var, self)
        if "." in var:
            return self.value(var)
        return self._row_value(i, var)

    def _row_value(self, i, name):
        raise NotImplementedError

    def _apply_between(self, cands, decl, i):
        between = decl["between"]
        val = self._match_value(between["value"], i)
        if is_missing(val):
            return []
        out = []
        for r in cands:
            lo = (
                r.get(_record_term_field(between["lower"]))
                if between.get("lower")
                else None
            )
            hi = (
                r.get(_record_term_field(between["upper"]))
                if between.get("upper")
                else None
            )
            if (between.get("lower") and is_missing(lo)) or (
                between.get("upper") and is_missing(hi)
            ):
                continue  # REQ-0128: missing bound -> ineligible
            if not comparable(val, lo or hi or val):
                _fail(
                    f"{self.e.lookup_paths[decl['id']]}.between",
                    "validation",
                    "incomparable_range_types",
                    "REQ-0121",
                    {
                        "intermediate": decl["id"],
                        "value_type": _expr._runtime_type_name(val),
                        "lower_type": _expr._runtime_type_name(lo),
                        "upper_type": _expr._runtime_type_name(hi),
                    },
                )
            if between.get("lower") and compare(lo, val) > 0:
                continue
            if between.get("upper") and compare(val, hi) > 0:
                continue
            out.append(r)
        return out

    def lookup_value(self, lid, col):
        decl = self.e.lookups_decl[lid]
        rec = self._match_lookup(decl, self._row_index())
        if rec is None:
            # REQ-0124: _match_lookup returns None only when no_match is declared.
            return decl["no_match"]
        cols = decl.get("columns")
        if cols is not None and col not in cols:
            _fail(
                self.where, "validation", "unknown_field", "REQ-0125", {"column": col}
            )
        if col not in rec:
            _fail(
                self.where, "validation", "unknown_field", "REQ-0125", {"column": col}
            )
        return rec[col]

    def window_value(self, kind, payload):
        e = self.e
        w = payload.get("window", {}) or {}
        group_by = w.get("group_by") or []
        order_by = w.get("order_by") or []
        filt = w.get("filter")
        n = self._win_n()
        if filt is not None:
            node, _ = _pred.parse(filt, self.where)
            eligible = [
                i
                for i in range(n)
                if _pred.evaluate(
                    node, lambda nm, i=i: self._win_val(e, i, nm), self.where
                )
                is True
            ]
        else:
            eligible = list(range(n))
        parts = {}
        for i in eligible:
            tup = tuple(_hashable(self._win_val(e, i, g)) for g in group_by)
            parts.setdefault(tup, []).append(i)
        result = [None] * n
        for idxs in parts.values():
            ordered = _order_indices(
                idxs,
                order_by,
                lambda i, t: self._win_val(
                    e, i, t if isinstance(t, str) else t["variable"]
                ),
                self.where,
            )
            try:
                vals = self._window_kind(kind, payload, ordered, e)
            except YamaaError as err:
                # A window failure belongs to its partition rather than to
                # one row, so it names the partition it could not answer.
                if "keys" not in err.context and err.phase != "validation":
                    part = {
                        g: _expr.json_value(self._win_val(e, idxs[0], g))
                        for g in group_by
                    }
                    err.context = {**err.context, "keys": [part]}
                raise
            for i, v in zip(ordered, vals):
                result[i] = v
        return result

    def _window_kind(self, kind, payload, ordered, e):
        if kind == "row_number":
            return list(range(1, len(ordered) + 1))
        if kind == "rank":
            method = payload.get("method", "competition")
            w = payload.get("window", {}) or {}
            terms = w.get("order_by") or []
            ranks = []
            cur = 0
            for k, i in enumerate(ordered):
                if k > 0 and not _order_terms_equal(
                    ordered[k - 1],
                    i,
                    terms,
                    lambda ii, t: self._win_val(
                        e, ii, t if isinstance(t, str) else t["variable"]
                    ),
                ):
                    cur = k if method == "competition" else cur + 1
                ranks.append(cur + 1)
            return ranks
        if kind == "row_value":
            offset = payload["offset"]
            src = payload["source"]
            out = []
            for k in range(len(ordered)):
                j = k + offset
                out.append(
                    self._win_val(e, ordered[j], src) if 0 <= j < len(ordered) else None
                )
            return out
        if kind == "previous_non_missing":
            src = payload["source"]
            out = []
            for k in range(len(ordered)):
                hit = None
                for j in range(k - 1, -1, -1):
                    v = self._win_val(e, ordered[j], src)
                    if not is_missing(v):
                        hit = v
                        break
                out.append(hit)
            return out
        if kind == "locf":
            src = payload["source"]
            out = []
            for k in range(len(ordered)):
                v = self._win_val(e, ordered[k], src)
                if not is_missing(v):
                    out.append(v)
                    continue
                hit = None
                for j in range(k - 1, -1, -1):
                    w = self._win_val(e, ordered[j], src)
                    if not is_missing(w):
                        hit = w
                        break
                out.append(hit)
            return out
        if kind == "baseline_flag":
            dv, rv = payload["date"], payload["reference_date"]
            cands = []
            for i in ordered:
                d = self._win_val(e, i, dv)
                r = self._win_val(e, i, rv)
                if is_missing(d) or is_missing(r):
                    continue
                if type(d) is not type(r):
                    _fail(
                        f"{self.where}.reference_date",
                        "validation",
                        "incompatible_input_type",
                        "REQ-0004",
                        {
                            "source": rv,
                            "expected": _expr._runtime_type_name(d),
                            "actual": _expr._runtime_type_name(r),
                        },
                    )
                if compare(d, r) <= 0:
                    cands.append((d, i))
            out = [None] * len(ordered)
            if cands:
                latest = max(c for c, _ in cands)
                winners = [i for c, i in cands if compare(c, latest) == 0]
                if len(winners) > 1:
                    _fail(
                        self.where,
                        "derivation",
                        "ambiguous_baseline",
                        "REQ-0322",
                        {
                            "date": _expr.json_value(latest),
                            "match_count": len(winners),
                        },
                    )
                out[ordered.index(winners[0])] = "Y"
            return out
        _fail(self.where, "validation", "unknown_field", "REQ-0321", {"window": kind})

    def aggregate_value(self, payload):
        if isinstance(payload, str):
            payload = {"expr": payload}
        if payload.get("derive"):
            return self._agg_derive(payload)
        node, names = _agg.parse(payload["expr"], f"{self.where}.expr")
        stars = {r[1] for r in _agg.collect_reductions(node, []) if r[0] == "starcount"}
        names = [n for n in names if n not in stars]
        qualified = [n for n in names if "." in n]
        plain = [n for n in names if "." not in n]
        i = self._row_index()
        if qualified and plain:
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "REQ-0504",
                {"expr": payload["expr"]},
            )
        if stars and not qualified:
            if len(stars) > 1:
                _fail(
                    self.where,
                    "validation",
                    "prohibited_construct",
                    "REQ-0504",
                    {"expr": payload["expr"]},
                )
            return self._agg_qualified(node, payload, sorted(stars))
        if qualified:
            return self._agg_qualified(node, payload, qualified)
        return self._agg_unqualified(node, payload, plain, i)

    def _agg_derive(self, payload):
        """Reduce over per-record derive bindings (REQ-1189/1190)."""
        e = self.e
        node, _names = _agg.parse(payload["expr"], f"{self.where}.expr")
        derive = payload["derive"]
        seen = set()
        for b in derive:
            n = b.get("name") if isinstance(b, dict) else None
            if n in seen:
                _fail(
                    self.where,
                    "validation",
                    "prohibited_construct",
                    "REQ-1189",
                    {"binding": n},
                )
            seen.add(n)
        qualifiers = set()
        for b in derive:
            _derive_qualifiers(b.get("derivation"), qualifiers)
        rel_quals = {q for q in qualifiers if q in e.inputs}
        inter_quals = {q for q in qualifiers if q in e.lookups_decl}
        if len(rel_quals) != 1:
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "REQ-1191",
                {"derive": [b.get("name") for b in derive if isinstance(b, dict)]},
            )
        for q in sorted(inter_quals):
            decl = e.lookups_decl[q]
            if not decl.get("keep") or not decl.get("order_by"):
                _fail(
                    self.where,
                    "validation",
                    "prohibited_construct",
                    "REQ-1242",
                    {"intermediate": q},
                )
        ds = next(iter(rel_quals))
        key = payload.get("key") or list(e.keys)
        if isinstance(key, str):
            key = [key]
        if isinstance(key, dict):
            key_fields = list(key.keys())
            key_exprs = list(key.values())
        else:
            key_fields = list(key)
            key_exprs = list(key)
        i = self._row_index()
        base_vals = [self._match_value(ke, i) for ke in key_exprs]
        matched = []
        if not any(is_missing(v) for v in base_vals):
            table = e.inputs[ds]
            matched = [
                r
                for r in table.records
                if all(
                    _hashable(r.get(kf)) == _hashable(bv)
                    for kf, bv in zip(key_fields, base_vals)
                )
            ]
        if payload.get("filter"):
            node_f, _ = _pred.parse(payload["filter"], self.where)
            matched = [
                r
                for r in matched
                if _pred.evaluate(
                    node_f, lambda n, r=r: r.get(n.split(".")[-1]), self.where
                )
                is True
            ]
        enriched = []
        var_names = set()
        for b in derive:
            _derive_var_names(b.get("derivation"), var_names)
        inter_scope = {}
        for name in sorted(var_names):
            head, _, field = name.partition(".")
            if head in inter_quals and name not in inter_scope:
                inter_scope[name] = self.lookup_value(head, field)
        for r in matched:
            scope = {}
            dctx = _DeriveCtx(e, ds, r, scope, self.where, i, inter_scope)
            for j, b in enumerate(derive):
                where = f"{self.where}.derive[{j}].derivation"
                bnode = _norm_derivation(b["derivation"], where)
                v = _expr.eval_at(bnode, dctx, where)
                binding = {"name": b["name"], "type": b["type"]}
                v = e._convert(v, binding, bnode, where, path=self.where)
                scope[b["name"]] = v
            for f, v in r.items():
                scope.setdefault(f"{ds}.{f}", v)
            enriched.append(scope)
        return _agg.eval_over_records(
            node, enriched, lambda s, n: s.get(n), {}, self.where, payload["expr"]
        )

    def _agg_qualified(self, node, payload, names):
        e = self.e
        ds = names[0].split(".")[0]
        if any(n.split(".")[0] != ds for n in names):
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "REQ-0504",
                {"expr": payload["expr"]},
            )
        table = e.inputs.get(ds)
        if table is None:
            _fail(
                self.where, "validation", "unknown_field", "REQ-0103", {"dataset": ds}
            )
        recs = table.records
        if payload.get("filter"):
            node_f, _ = _pred.parse(payload["filter"], self.where)
            recs = [
                r
                for r in recs
                if _pred.evaluate(
                    node_f, lambda n, r=r: r.get(n.split(".")[-1]), self.where
                )
                is True
            ]
        i = self._row_index()

        def field_of(r, name):
            return r.get(name.split(".")[-1])

        group_by = payload.get("group_by")
        if group_by:
            gcols = [g.split(".")[-1] for g in group_by]
            for g in gcols:
                if g not in e.keys:
                    _fail(
                        self.where,
                        "validation",
                        "prohibited_construct",
                        "REQ-0507",
                        {"group_by": g},
                    )
            parts = {}
            for r in recs:
                tup = tuple(_hashable(r.get(c)) for c in gcols)
                parts.setdefault(tup, []).append(r)
            tup = tuple(_hashable(e.rows[i].get(c)) for c in gcols)
            grec = parts.get(tup, [])
            if payload.get("between"):
                grec = self._agg_between(grec, payload["between"], i, ds)
            consts = {
                n: e.rows[i].get(n.split(".")[-1])
                for n in names
                if n.split(".")[-1] in gcols
            }
            return _agg.eval_over_records(
                node, grec, field_of, consts, self.where, payload["expr"]
            )
        key = payload.get("key")
        if key is not None:
            if isinstance(key, str):
                key = [key]
            if isinstance(key, dict):
                key_fields = list(key.keys())
                key_exprs = list(key.values())
            else:
                key_fields = list(key)
                key_exprs = list(key)
            mvals = [self._match_value(v, i) for v in key_exprs]
            if any(is_missing(v) for v in mvals):
                return None
            matched = [
                r
                for r in recs
                if all(
                    not is_missing(r.get(k))
                    and comparable(mv, r.get(k))
                    and compare(mv, r.get(k)) == 0
                    for k, mv in zip(key_fields, mvals)
                )
            ]
            if payload.get("between"):
                matched = self._agg_between(matched, payload["between"], i, ds)
            consts = {}
            return _agg.eval_over_records(
                node, matched, field_of, consts, self.where, payload["expr"]
            )
        consts = {}
        applicable = [k for k in e.keys if k in table.fields]
        if applicable:
            parts = {}
            for r in recs:
                tup = tuple(_hashable(r.get(c)) for c in applicable)
                parts.setdefault(tup, []).append(r)
            tup = tuple(_hashable(e.rows[i].get(c)) for c in applicable)
            recs = parts.get(tup, [])
        if payload.get("between"):
            recs = self._agg_between(recs, payload["between"], i, ds)
        return _agg.eval_over_records(
            node, recs, field_of, consts, self.where, payload["expr"]
        )

    def _agg_between(self, recs, between, i, ds):
        val = self._match_value(between["value"], i)
        if is_missing(val):
            return []
        out = []
        for r in recs:
            lo = (
                r.get(_record_term_field(between["lower"]))
                if between.get("lower")
                else None
            )
            hi = (
                r.get(_record_term_field(between["upper"]))
                if between.get("upper")
                else None
            )
            if (between.get("lower") and is_missing(lo)) or (
                between.get("upper") and is_missing(hi)
            ):
                continue
            if between.get("lower") and compare(lo, val) > 0:
                continue
            if between.get("upper") and compare(val, hi) > 0:
                continue
            out.append(r)
        return out

    def _agg_unqualified(self, node, payload, names, i):
        e = self.e
        group_by = payload.get("group_by")
        if not group_by:
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "REQ-0507",
                {"expr": payload["expr"]},
            )
        if payload.get("filter"):
            node_f, _ = _pred.parse(payload["filter"], self.where)
            idxs = [
                j
                for j in range(len(e.rows))
                if _pred.evaluate(node_f, lambda n, j=j: e.rows[j].get(n), self.where)
                is True
            ]
        else:
            idxs = list(range(len(e.rows)))
        parts = {}
        for j in idxs:
            tup = tuple(_hashable(e.rows[j].get(g)) for g in group_by)
            parts.setdefault(tup, []).append(j)
        tup = tuple(_hashable(e.rows[i].get(g)) for g in group_by)
        members = parts.get(tup, [])
        recs = [e.rows[j] for j in members]
        consts = {n: e.rows[i].get(n) for n in names if n in group_by}
        return _agg.eval_over_records(
            node, recs, lambda r, n: r.get(n), consts, self.where, payload["expr"]
        )


class _ColCtx(_BaseCtx):
    """Column-phase context: unqualified names read completed output columns."""

    def __init__(self, engine, i, where):
        super().__init__(engine, where)
        self.i = i
        self._col_phase = True  # REQ-0075 applies to column derivations

    def _row_index(self):
        return self.i

    def _row_value(self, i, name):
        return self.e.rows[i].get(name)

    def _driver_intermediate(self):
        origin = self.e._origins[self.i]
        return origin if origin in self.e.drivers else None

    def value(self, name):
        e = self.e
        if "." in name:
            parts = name.split(".")
            if self._is_lookup(parts[0]):
                return self.lookup_value(parts[0], parts[1])
            # REQ-0075 for the row's own records, REQ-0127 for a joined read.
            return _expr._one_record(name, None, _ABSENT, None, self)
        row = e.rows[self.i]
        if name not in row:
            origin = e._origins[self.i]
            table = self._table(origin) if origin else None
            if table is not None and name in table.fields:
                _fail(
                    self.where,
                    "validation",
                    "unresolvable_name",
                    "REQ-0189",
                    {"identifier": name, "suggestion": f"{origin}.{name}"},
                )
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0070",
                {"identifier": name},
            )
        return row[name]

    def source_records(self, var):
        e = self.e
        parts = var.split(".")
        ds = parts[0]
        i = self.i
        if self._is_lookup(ds):
            rec = self._match_lookup(e.lookups_decl[ds], i)
            return [rec] if rec is not None else []
        table = self._table(ds)
        if table is None:
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0103",
                {"identifier": var},
            )
        field = parts[-1]
        if field not in table.fields:
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0103",
                {"identifier": var},
            )
        origin = e._origins[i]
        if ds == origin:
            recs = list(e._recs[i].get(ds, []))
            return recs
        applicable = [k for k in e.keys if k in table.fields]
        if not applicable:
            _fail(
                self.where,
                "validation",
                "no_applicable_keys",
                "REQ-0152",
                {"variable": var},
            )
        row = e.rows[i]
        return e._join_records(table, applicable, [row.get(k) for k in applicable])

    def compute_ident(self, name):
        if "." in name:
            parts = name.split(".")
            if self._is_lookup(parts[0]):
                return self.lookup_value(parts[0], parts[1])
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "REQ-0442",
                {"identifier": name},
            )
        return self.value(name)

    def odm_value(self, payload):
        """REQ-1270: a column derivation reads the scope of the row
        template that built the row."""
        e, i = self.e, self.i
        origin = e._origins[i]
        return e._odm_read(
            payload,
            origin,
            e._built[i],
            e._recs[i].get(origin, []),
            e.rows[i],
            self.where,
            "derivation",
        )


class _RowCtx(_BaseCtx):
    """Row-construction context: record-driven or grouped."""

    def __init__(
        self, engine, phase, template, ds, record=None, group=None, groupkeys=None
    ):
        super().__init__(engine, "<row>")
        self.phase = phase
        self.template = template
        self.ds = ds
        self.record = record
        self.group = group
        self.groupkeys = groupkeys or {}
        self.row = {}

    def _row_index(self):
        _fail(self.where, "validation", "phase_boundary", "REQ-0126", {})

    def _row_value(self, i, name):
        _fail(self.where, "validation", "phase_boundary", "REQ-0126", {})

    def _driver_intermediate(self):
        return self.ds if self.ds in self.e.drivers else None

    def value(self, name):
        if "." in name:
            parts = name.split(".")
            if self._is_lookup(parts[0]):
                decl = self.e.lookups_decl[parts[0]]
                key_fields, key_exprs = self._lookup_keys(decl)
                match_vals = [self._row_match_value(v) for v in key_exprs]
                rec = self._match_lookup_row(decl, key_fields, match_vals)
                if rec is None:
                    return decl["no_match"]
                col = parts[1]
                cols = decl.get("columns")
                if (cols is not None and col not in cols) or col not in rec:
                    _fail(
                        self.where,
                        "validation",
                        "unknown_field",
                        "REQ-0125",
                        {"column": col},
                    )
                return rec.get(col)
            # REQ-0156: the implicit join binds one value per row.
            return _expr._one_record(name, None, _ABSENT, None, self)
        if name in self.row:
            return self.row[name]
        if name in self.groupkeys:
            return self.groupkeys[name]
        table = self._table(self.ds)
        if table is not None and name in table.fields:
            _fail(
                self.where,
                "validation",
                "unresolvable_name",
                "REQ-0189",
                {"identifier": name, "suggestion": f"{self.ds}.{name}"},
            )
        _fail(
            self.where, "validation", "unknown_field", "REQ-0070", {"identifier": name}
        )

    def source_records(self, var):
        parts = var.split(".")
        ds = parts[0]
        if self._is_lookup(ds):
            decl = self.e.lookups_decl[ds]
            key_fields, key_exprs = self._lookup_keys(decl)
            match_vals = [self._row_match_value(v) for v in key_exprs]
            rec = self._match_lookup_row(decl, key_fields, match_vals)
            return [rec] if rec is not None else []
        table = self._table(ds)
        if table is None:
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0103",
                {"identifier": var},
            )
        if len(parts) >= 3 and parts[1] not in table.fields:
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0103",
                {"identifier": var},
            )
        if ds == self.ds:
            if self.record is not None:
                return [self.record]
            if parts[1] in self.groupkeys:
                return [{parts[1]: self.groupkeys[parts[1]]}]
            _fail(
                self.where,
                "validation",
                "ungrouped_driver_field",
                "REQ-0067",
                {"variable": var},
            )
        applicable = [k for k in self.e.keys if k in table.fields]
        if not applicable:
            _fail(
                self.where,
                "validation",
                "no_applicable_keys",
                "REQ-0152",
                {"variable": var},
            )
        anchor = self.record if self.record is not None else self.groupkeys
        for k in applicable:
            if self.group is not None and k not in self.groupkeys:
                _fail(
                    self.where,
                    "validation",
                    "ungrouped_driver_field",
                    "REQ-0067",
                    {"key": k},
                )
        return self.e._join_records(
            table, applicable, [anchor.get(k) for k in applicable]
        )

    def record_predicate(self, text, record, default_ds=None):
        record = dict(record)
        record["_ds"] = self.ds
        return super().record_predicate(text, record)

    def order_records(self, recs, order_by):
        return _order_records(recs, order_by, self.where)

    def _row_match_value(self, var):
        """REQ-0126 match-value resolution during row construction."""
        if isinstance(var, dict):
            return _expr.eval_expr(var, self)
        if "." in var:
            head, _, field = var.partition(".")
            if head == self.ds:
                if self.record is not None:
                    if field not in self.record:
                        _fail(
                            self.where,
                            "validation",
                            "unknown_field",
                            "REQ-0103",
                            {"identifier": var},
                        )
                    return self.record.get(field)
                if field in self.groupkeys:
                    return self.groupkeys[field]
            _fail(
                self.where,
                "validation",
                "phase_boundary",
                "REQ-0126",
                {"variable": var},
            )
        if var in self.row:
            return self.row[var]
        if var in self.groupkeys:
            return self.groupkeys[var]
        if var in ((self.template or {}).get("derivations") or {}):
            _fail(
                self.where,
                "validation",
                "phase_boundary",
                "REQ-0126",
                {"variable": var},
            )
        if self.record is not None and var in self.record:
            return self.record[var]
        _fail(
            self.where, "validation", "unknown_field", "REQ-0070", {"identifier": var}
        )

    def _match_operand(self, name):
        return self._row_match_value(name)

    def _row_driver_record(self, driver):
        if driver == self.ds:
            if self.record is not None:
                return self.record
            return dict(self.groupkeys)
        return None

    def _match_lookup_row(self, decl, key, match_vals):
        """Row-phase lookup: match values are pre-resolved; the cache key
        gains the driver record when the filter is correlated."""
        corr = self._correlated_filter(decl)
        ck = (decl["id"], tuple(_hashable(v) for v in match_vals))
        if corr is not None:
            _, driver_c, _ = corr
            drec_c = self._row_driver_record(driver_c)
            ck = (
                ck,
                tuple(sorted((k, _hashable(v)) for k, v in drec_c.items()))
                if drec_c
                else (),
            )
        if ck in self.e._lookups:
            return self.e._lookups[ck]
        recs = self._eligible(decl)
        cands = []
        if not any(is_missing(v) for v in match_vals):
            for r in recs:
                ok = True
                for k, mv in zip(key, match_vals):
                    rv = r.get(k)
                    if is_missing(rv) or not comparable(mv, rv) or compare(mv, rv) != 0:
                        ok = False
                        break
                if ok:
                    cands.append(r)
            if decl.get("between"):
                cands = self._apply_between(cands, decl, -1)
            if corr is not None:
                node, driver, donor = corr
                drec = self._row_driver_record(driver)

                def resolve(name, _r=None):
                    parts = name.split(".")
                    if donor == "SELF":
                        if len(parts) == 2 and parts[0] == "SELF":
                            return _r.get(parts[1])
                        if len(parts) == 1:
                            return _r.get(name)
                    elif len(parts) == 2 and parts[0] == donor:
                        return _r.get(parts[1])
                    if len(parts) == 2 and parts[0] == driver:
                        return drec.get(parts[1]) if drec is not None else None
                    _fail(
                        self.where,
                        "validation",
                        "unknown_field",
                        "REQ-0132",
                        {"identifier": name},
                    )

                cands = [
                    r
                    for r in cands
                    if _pred.evaluate(node, lambda n, _r=r: resolve(n, _r), self.where)
                    is True
                ]
            if len(cands) > 1:
                ob, keep = decl.get("order_by"), decl.get("keep")
                if not ob or not keep:
                    # REQ-0145: at the join phase, before any column.
                    ctx = self._match_context(decl, key, match_vals, None)
                    ctx["match_count"] = len(cands)
                    _fail(
                        self.e.lookup_paths[decl["id"]],
                        "join",
                        "multiple_matches",
                        "REQ-0127",
                        ctx,
                    )
                cands = _order_records(cands, ob, self.where)
                cands = [cands[0] if keep == "first" else cands[-1]]
        if not cands:
            # REQ-0124: no_match answers the read; without it, fail.
            if "no_match" in decl:
                self.e._lookups[ck] = None
                return None
            _fail(
                self.e.lookup_paths[decl["id"]],
                "join",
                "unmatched_key",
                "REQ-0124",
                self._match_context(decl, key, match_vals, None),
            )
        self.e._lookups[ck] = cands[0]
        return cands[0]

    def _match_value(self, var, i):
        return self._row_match_value(var)

    def window_value(self, kind, payload):
        _fail(self.where, "validation", "phase_boundary", "REQ-0326", {})

    def aggregate_value(self, payload):
        if self.group is None:
            _fail(self.where, "validation", "prohibited_construct", "REQ-0508", {})
        if isinstance(payload, str):
            payload = {"expr": payload}
        node, names = _agg.parse(payload["expr"], f"{self.where}.expr")
        ds = self.ds
        for n in names:
            if "." in n and n.split(".")[0] != ds:
                _fail(
                    self.where,
                    "validation",
                    "prohibited_construct",
                    "REQ-0508",
                    {"identifier": n},
                )
        if payload.get("filter"):
            node_f, _ = _pred.parse(payload["filter"], self.where)
            recs = [
                r
                for r in self.group
                if _pred.evaluate(
                    node_f, lambda n, r=r: r.get(n.split(".")[-1]), self.where
                )
                is True
            ]
        else:
            recs = list(self.group)
        consts = {
            n: self.groupkeys[n.split(".")[-1]]
            for n in names
            if "." in n
            and n.split(".")[-1] in self.groupkeys
            or "." not in n
            and n in self.groupkeys
        }
        try:
            return _agg.eval_over_records(
                node,
                recs,
                lambda r, n: r.get(n.split(".")[-1]),
                consts,
                self.where,
                payload["expr"],
            )
        except YamaaError as e:
            if e.condition == "aggregate_multiple_records":
                # REQ-0501: a grouped row's ONLY fails while its row is
                # constructed, naming the template and the group.
                raise YamaaError(
                    phase="row_construction",
                    condition=e.condition,
                    requirement=e.requirement,
                    spec_paths=e.spec_paths,
                    context={
                        **e.context,
                        "row": (self.template or {}).get("id"),
                        "group": dict(self.groupkeys),
                    },
                ) from e
            raise

    def compute_ident(self, name):
        if "." in name:
            parts = name.split(".")
            if parts[0] == self.ds:
                if self.record is not None:
                    return self.record.get(parts[1])
                if parts[1] in self.groupkeys:
                    return self.groupkeys[parts[1]]
                _fail(
                    self.where,
                    "validation",
                    "ungrouped_driver_field",
                    "REQ-0067",
                    {"identifier": name},
                )
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "REQ-0442",
                {"identifier": name},
            )
        return self.value(name)

    def odm_value(self, payload):
        if self.group is not None:
            built, recs = ("group", self.template), self.group
        else:
            built, recs = ("record", self.template), [self.record]
        return self.e._odm_read(
            payload,
            self.ds,
            built,
            recs,
            self.row,
            self.where,
            "row_construction",
        )


class _RowWinCtx(_BaseCtx):
    """Row-template window phase (REQ-0326): windows over one template's"""

    def __init__(self, engine, template, ds, rows, recs):
        super().__init__(engine, f"{engine.row_path(template)}.derivations")
        self.template = template
        self.ds = ds
        self.trows = rows
        self.recs = recs  # per-row input records, or None (grouped)

    def _win_n(self):
        return len(self.trows)

    def _win_val(self, e, i, var):
        if "." in var:
            parts = var.split(".")
            if parts[0] == self.ds:
                if self.recs is not None:
                    return self.recs[i].get(parts[1])
                _fail(
                    self.where,
                    "validation",
                    "ungrouped_driver_field",
                    "REQ-0067",
                    {"identifier": var},
                )
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0070",
                {"identifier": var},
            )
        if var in self.trows[i]:
            return self.trows[i][var]
        _fail(
            self.where, "validation", "unknown_field", "REQ-0070", {"identifier": var}
        )


class _KeyCtx(_BaseCtx):
    """Key-table derivation: per input record, base-qualified sources only."""

    def __init__(self, engine, base, record):
        super().__init__(engine, "<key>")
        self.base = base
        self.record = record

    def _row_index(self):
        _fail(self.where, "validation", "phase_boundary", "REQ-0069", {})

    def _row_value(self, i, name):
        _fail(self.where, "validation", "phase_boundary", "REQ-0069", {})

    def value(self, name):
        parts = name.split(".")
        if len(parts) == 2 and parts[0] == self.base:
            return self.record.get(parts[1])
        _fail(self.where, "validation", "forward_reference", "REQ-0074", {"name": name})

    def source_records(self, var):
        parts = var.split(".")
        if len(parts) == 2 and parts[0] == self.base:
            return [self.record]
        _fail(
            self.where, "validation", "forward_reference", "REQ-0074", {"variable": var}
        )

    def record_predicate(self, text, record):
        record = dict(record)
        record["_ds"] = self.base
        return super().record_predicate(text, record)

    def compute_ident(self, name):
        if "." in name and name.split(".")[0] == self.base:
            return self.record.get(name.split(".")[1])
        _fail(
            self.where,
            "validation",
            "forward_reference",
            "REQ-0074",
            {"identifier": name},
        )

    def window_value(self, kind, payload):
        _fail(self.where, "validation", "forward_reference", "REQ-0074", {})

    def aggregate_value(self, payload):
        _fail(self.where, "validation", "forward_reference", "REQ-0074", {})

    def odm_value(self, payload):
        # REQ-1269: the scope is the records a key combination was derived
        # from, so no key can be derived from it.
        _fail(self.where, "validation", "forward_reference", "REQ-0074", {})


class _KeyWinCtx(_BaseCtx):
    """Phase-B key derivation: windows over the phase-A key table."""

    def __init__(self, engine, base, krows, krecs, where):
        super().__init__(engine, where)
        self.base = base
        self.krows = krows
        self.krecs = krecs

    def _win_n(self):
        return len(self.krows)

    def _win_val(self, e, i, var):
        if "." in var:
            parts = var.split(".")
            if parts[0] == self.base:
                return self.krecs[i].get(parts[1])
            _fail(
                self.where,
                "validation",
                "forward_reference",
                "REQ-0074",
                {"identifier": var},
            )
        if var in self.krows[i]:
            return self.krows[i][var]
        _fail(
            self.where,
            "validation",
            "forward_reference",
            "REQ-0074",
            {"identifier": var},
        )

    def value(self, name):
        _fail(self.where, "validation", "forward_reference", "REQ-0074", {"name": name})

    def source_records(self, var):
        _fail(
            self.where, "validation", "forward_reference", "REQ-0074", {"variable": var}
        )


class _DeriveCtx:
    """Evaluation scope for one record's derive bindings (REQ-1189)."""

    def __init__(self, engine, ds, record, scope, where, i, inter=None):
        self.e = engine
        self.ds = ds
        self.record = record
        self.scope = scope
        self.where = where
        self.i = i
        self.inter = inter or {}

    def value(self, name):
        if "." in name:
            d, _, f = name.partition(".")
            if d != self.ds:
                if name in self.inter:
                    return self.inter[name]
                _fail(
                    self.where,
                    "validation",
                    "unknown_field",
                    "REQ-0103",
                    {"identifier": name},
                )
            if f not in self.record:
                _fail(
                    self.where,
                    "validation",
                    "unknown_field",
                    "REQ-0103",
                    {"identifier": name},
                )
            return self.record[f]
        if name in self.scope:
            return self.scope[name]
        _fail(
            self.where, "validation", "unknown_field", "REQ-1189", {"identifier": name}
        )

    def source_records(self, var):
        d, _, _ = var.partition(".")
        if d != self.ds:
            if var in self.inter:
                return [{var.split(".")[-1]: self.inter[var]}]
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0103",
                {"identifier": var},
            )
        return [self.record]

    def record_predicate(self, text, record, default_ds=None):
        node, _ = _pred.parse(text, self.where)
        ds = record.get("_ds") or default_ds

        def resolve(name):
            parts = name.split(".")
            if len(parts) == 2 and parts[0] == ds:
                return record.get(parts[1])
            if len(parts) == 1:
                return record.get(name)
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-0189",
                {"identifier": name},
            )

        return _pred.evaluate(node, resolve, self.where)

    def value_field(self, var):
        return var.split(".")[-1]

    def lookup_value(self, lid, col):
        _fail(
            self.where,
            "validation",
            "prohibited_construct",
            "REQ-1191",
            {"identifier": lid},
        )

    def aggregate_value(self, payload):
        _fail(self.where, "validation", "prohibited_construct", "REQ-1191", {})

    def odm_value(self, payload):
        _odm_no_scope(self.where, payload)

    def compute_ident(self, name):
        return self.value(name)


class _DonorCtx(_BaseCtx):
    """REQ-1185/REQ-1263: one donor record being augmented. A bare name, or
    one qualified by the intermediate's dataset, reads the record's stored
    fields and earlier derivations. A name qualified by another intermediate
    reads that intermediate's selection, matched from this donor record and
    shared by every derivation of the record."""

    def __init__(self, engine, decl, record, j, where):
        super().__init__(engine, where)
        self.decl = decl
        self.ds = decl["dataset"]
        self.record = record
        self.j = j

    def _row_index(self):
        return ("donor", self.decl["id"], self.j)

    def _row_value(self, i, name):
        return self._field(name)

    def _driver_ds(self):
        return self.ds

    def _correlated_record(self, driver, i):
        return self.record

    def _field(self, name):
        if name not in self.record:
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "REQ-1185",
                {"identifier": name},
            )
        return self.record[name]

    def _other(self, head):
        other = self.e.lookups_decl.get(head)
        if other is None or other is self.decl:
            return None
        return other

    def value(self, name):
        head, dot, field = name.partition(".")
        if not dot:
            return self._field(name)
        if head == self.ds:
            return self._field(field)
        if self._other(head) is not None:
            return self.lookup_value(head, field)
        _fail(
            self.where, "validation", "unknown_field", "REQ-1185", {"identifier": name}
        )

    def source_records(self, var):
        head, _, field = var.partition(".")
        if head == self.ds:
            self._field(field)
            return [self.record]
        other = self._other(head)
        if other is not None:
            self.lookup_value(head, field)  # REQ-0125: a readable column
            rec = self._match_lookup(other, self._row_index())
            return [rec] if rec is not None else []
        _fail(
            self.where, "validation", "unknown_field", "REQ-1185", {"identifier": var}
        )

    def compute_ident(self, name):
        return self.value(name)


class _DonorWinCtx(_BaseCtx):
    """REQ-1185: window evaluation over intermediate donor records."""

    def __init__(self, engine, ds, recs, where):
        super().__init__(engine, where)
        self.ds = ds
        self.recs = recs

    def _win_n(self):
        return len(self.recs)

    def _win_val(self, e, i, var):
        if "." in var:
            d, _, f = var.partition(".")
            if d != self.ds:
                _fail(
                    self.where,
                    "validation",
                    "unknown_field",
                    "REQ-0103",
                    {"identifier": var},
                )
            return self.recs[i].get(f)
        return self.recs[i].get(var)


def _order_term_value(term, get):
    if isinstance(term, str):
        return get(term), "asc", "last"
    return (
        get(term["variable"]),
        term.get("direction", "asc"),
        term.get("nulls", "last"),
    )


def _order_cmp(ta, tb, get_a, get_b, where):
    for term in ta[1]:
        va, da, na = _order_term_value(term, get_a)
        vb, db, nb = _order_term_value(term, get_b)
        assert da == db and na == nb
        ma, mb = is_missing(va), is_missing(vb)
        if ma and mb:
            continue
        if ma or mb:
            first = na == "first"
            return -1 if (ma and first) or (mb and not first) else 1
        if not comparable(va, vb):
            _fail(
                where,
                "validation",
                "incompatible_input_type",
                "REQ-0323",
                {"term": term},
            )
        c = compare(va, vb)
        if c:
            return c if da == "asc" else -c
    return 0


def _order_indices(idxs, order_by, get, where):
    if not order_by:
        return list(idxs)

    def get_i(i, t):
        return get(i, t)

    return sorted(
        idxs,
        key=cmp_to_key(
            lambda a, b: _order_cmp(
                (a, order_by),
                (b, order_by),
                lambda t: get(a, t),
                lambda t: get(b, t),
                where,
            )
        ),
    )


def _record_term_field(term):
    """Field name an order_by term addresses on an input record."""
    name = term if isinstance(term, str) else term["variable"]
    return name.split(".")[-1]


def _order_records(recs, order_by, where):
    idxs = list(range(len(recs)))
    ordered = _order_indices(
        idxs, order_by, lambda i, t: recs[i].get(_record_term_field(t)), where
    )
    return [recs[i] for i in ordered]


def _order_rows(rows, order_by, get, where):
    idxs = list(range(len(rows)))
    ordered = _order_indices(idxs, order_by, lambda i, t: get(rows[i], t), where)
    return [rows[i] for i in ordered]


def _order_terms_equal(a, b, terms, get):
    for term in terms:
        va = get(a, term if isinstance(term, str) else term["variable"])
        vb = get(b, term if isinstance(term, str) else term["variable"])
        if is_missing(va) or is_missing(vb):
            if not (is_missing(va) and is_missing(vb)):
                return False
            continue
        if not comparable(va, vb) or compare(va, vb) != 0:
            return False
    return True


class _ConvertFail(Exception):
    def __init__(self, requirement):
        super().__init__(requirement)
        self.requirement = requirement


def _convert_value(v, target):
    if is_missing(v):
        return None
    if isinstance(v, bool):
        raise _ConvertFail("REQ-0013")  # bool never converts
    if target == "str":
        if isinstance(v, str):
            return v
        if isinstance(v, int):
            return str(v)
        if isinstance(v, float):
            return float_text(v)
        if isinstance(v, YDateTime):
            return datetime_text(v)
        if isinstance(v, YDate):
            return date_text(v)
        raise _ConvertFail("REQ-0013")
    if target == "int":
        if isinstance(v, int):
            return v
        if isinstance(v, float):
            if v.is_integer() and INT64_MIN <= v <= INT64_MAX:
                return int(v)
            raise _ConvertFail("REQ-0021")
        if isinstance(v, str):
            try:
                return parse_int_text(v)
            except ValueError:
                pass
            try:
                f = parse_float_text(v)
            except ValueError:
                raise _ConvertFail("REQ-0021")
            if f is None:
                return None
            if f.is_integer() and INT64_MIN <= f <= INT64_MAX:
                return int(f)
            raise _ConvertFail("REQ-0021")
        raise _ConvertFail("REQ-0013")
    if target == "float":
        if isinstance(v, int):
            return normalize_number(float(v))
        if isinstance(v, float):
            return v
        if isinstance(v, str):
            try:
                return parse_float_text(v)
            except ValueError:
                raise _ConvertFail("REQ-0013")
        raise _ConvertFail("REQ-0013")
    if target == "date":
        if isinstance(v, YDate) and not isinstance(v, YDateTime):
            return v
        if isinstance(v, str):
            try:
                return parse_date(v)
            except ValueError:
                raise _ConvertFail("REQ-0601")
        raise _ConvertFail("REQ-0013")
    if target == "datetime":
        if isinstance(v, YDateTime):
            return v
        if isinstance(v, str):
            try:
                return parse_datetime(v)
            except ValueError:
                raise _ConvertFail("REQ-0601")
        raise _ConvertFail("REQ-0013")
    raise _ConvertFail("REQ-0013")
