"""Clean-room derivation engine: spec loading, row construction (R001),"""

import copy
import csv
import io
import json
import os
import re
from functools import cmp_to_key

import yaml

from . import agg as _agg
from . import expr as _expr
from . import pred as _pred
from . import validate as _validate
from .csv_io import read_csv, read_parquet, write_csv_text
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


_CATALOG_IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_CATALOG_PLACEHOLDER = re.compile(r"\$\{([^}]*)\}")
_CATALOG_INT = re.compile(r"^[+-]?[0-9]+$")


def _only_error(e, where):
    """REQ-0501: ONLY over several records in the column phase fails as
    derivation/aggregate_multiple_records. Pass through anything else."""
    if e.phase == "row_construction" and e.condition == "multiple_values_per_key":
        return YamaaError(
            phase="derivation",
            condition="aggregate_multiple_records",
            requirement="REQ-0501",
            spec_paths=[where],
            context={
                "reducer": "ONLY",
                "record_count": (e.context or {}).get("record_count"),
            },
        )
    return e


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
            _fail("root", "validation", "invalid_field_type", "R006", {})
        self._expand_catalogs()
        self._expand_named_windows()
        if s.get("schema_version") != "1.0":
            _fail("schema_version", "validation", "schema_version_mismatch", "R006", {})
        if s.get("parents"):
            self._reject_parents(s["parents"])
        self.functions = None
        if project_root is not None and _uses_function(s):
            from . import functions as _functions

            self.functions = _functions.load_environment(project_root)
        for req in ("domain", "keys", "input", "output", "columns"):
            if req not in s:
                _fail(
                    req, "validation", "missing_required_field", "R006", {"field": req}
                )
        self.domain = s["domain"]
        self.keys = list(s["keys"])
        self.inputs = {}
        for name, decl in s["input"].items():
            self.inputs[name] = self._load_input(name, decl)
        self.lookups_decl = {d["id"]: d for d in s.get("intermediates", []) or []}
        self.colspecs = {}
        self.col_order = []
        for c in s["columns"]:
            if c["name"] in self.colspecs:
                _fail(
                    f"columns.{c['name']}",
                    "validation",
                    "duplicate_identifier",
                    "R005-38",
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
        self._lookups = {}  # (lid, row_idx) -> record | None
        self._eligible = {}  # lid -> eligible records (computed once)
        self._dict_cache = {}  # written dict_yaml path -> loaded dict
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

    def _expand_catalogs(self):
        """Expand row catalogs into ordinary row templates. A template"""
        s = self.spec
        rows = s.get("rows") or []
        if not any(isinstance(t, dict) and "catalog" in t for t in rows):
            return
        out = []
        for i, t in enumerate(rows):
            if not isinstance(t, dict) or "catalog" not in t:
                out.append(t)
                continue
            out.extend(self._expand_catalog(t, i))
        s["rows"] = out

    def _catalog_fail(
        self, where, reason, condition="invalid_row_catalog", context=None
    ):
        ctx = {"reason": reason}
        if context:
            ctx.update(context)
        _fail(where, "validation", condition, "REQ-1249", ctx)

    def _expand_catalog(self, t, i):
        where = f"rows[{i}].catalog"
        cat = t.get("catalog")
        tid = t.get("id")
        if not isinstance(cat, dict):
            self._catalog_fail(where, "invalid_declaration")
        if not isinstance(tid, str) or not tid:
            self._catalog_fail(where, "invalid_template_id")
        path = cat.get("path")
        if not isinstance(path, str) or not path:
            self._catalog_fail(where, "invalid_path")
        full = os.path.normpath(os.path.join(self.spec_dir, path))
        if full != self.spec_dir and not full.startswith(self.spec_dir + os.sep):
            self._catalog_fail(where, "invalid_path")
        try:
            with open(full, "rb") as f:
                raw = f.read()
        except OSError:
            self._catalog_fail(where, "invalid_path")
        try:
            text = raw.decode("ascii")
        except UnicodeDecodeError:
            self._catalog_fail(where, "non_ascii_catalog")
        id_column = cat.get("id_column")
        if not isinstance(id_column, str) or not _CATALOG_IDENT.match(id_column):
            self._catalog_fail(where, "invalid_id_column")
        types = cat.get("types") or {}
        if not isinstance(types, dict):
            self._catalog_fail(where, "invalid_types")
        for f, typ in types.items():
            if typ not in ("str", "int", "float"):
                self._catalog_fail(where, "invalid_types")
        unique_columns = cat.get("unique_columns") or []
        if not isinstance(unique_columns, list) or any(
            not isinstance(c, str) for c in unique_columns
        ):
            self._catalog_fail(where, "invalid_unique_columns")

        parsed = list(csv.reader(io.StringIO(text)))
        if not parsed:
            self._catalog_fail(where, "empty_catalog")
        header = parsed[0]
        if len(set(header)) != len(header) or any(
            not _CATALOG_IDENT.match(h or "") for h in header
        ):
            self._catalog_fail(where, "invalid_header")
        data = parsed[1:]
        if not data:
            self._catalog_fail(where, "empty_catalog")
        if id_column not in header:
            self._catalog_fail(where, "invalid_id_column")
        for f in types:
            if f not in header:
                self._catalog_fail(where, "invalid_types")
        for c in unique_columns:
            if c not in header:
                self._catalog_fail(where, "invalid_unique_columns")

        records = []
        for r in data:
            if len(r) != len(header):
                self._catalog_fail(where, "ragged_record")
            rec = {}
            for h, cell in zip(header, r):
                if cell == "":
                    self._catalog_fail(where, "missing_value")
                rec[h] = self._catalog_cell(where, h, cell, types.get(h, "str"))
            records.append(rec)
        ids = [str(r[id_column]) for r in records]
        if len(set(ids)) != len(ids):
            self._catalog_fail(where, "duplicate_id_values")
        for c in unique_columns:
            vals = [str(r[c]) for r in records]
            if len(set(vals)) != len(vals):
                self._catalog_fail(
                    where, "duplicate_unique_values", context={"column": c}
                )

        generated = []
        for rec in records:
            nt = copy.deepcopy(t)
            del nt["catalog"]
            nt["id"] = f"{tid}_{rec[id_column]}"
            if isinstance(nt.get("filter"), str):
                nt["filter"] = self._sub_placeholders(
                    nt["filter"], where, rec, types, header, True
                )
            if isinstance(nt.get("derivations"), dict):
                nt["derivations"] = {
                    k: self._sub_placeholders(v, where, rec, types, header, False)
                    for k, v in nt["derivations"].items()
                }
            generated.append(nt)
        return generated

    def _catalog_cell(self, where, field, cell, typ):
        if typ == "str":
            return cell
        if typ == "int":
            if _CATALOG_INT.match(cell):
                return int(cell)
            self._catalog_fail(
                where, "invalid_int_cell", context={"field": field, "value": cell}
            )
        try:
            v = float(cell)
        except ValueError:
            v = None
        if v is None or v != v or v in (float("inf"), float("-inf")):  # noqa: PLR0124 -- NaN check is the intent
            self._catalog_fail(
                where, "invalid_float_cell", context={"field": field, "value": cell}
            )
        return v

    def _sub_placeholders(self, node, where, rec, types, header, pred):
        """Replace `${FIELD}` placeholders for one catalog record. In a"""
        if isinstance(node, dict):
            return {
                k: self._sub_placeholders(
                    v, where, rec, types, header, k in ("filter", "when")
                )
                for k, v in node.items()
            }
        if isinstance(node, list):
            return [
                self._sub_placeholders(v, where, rec, types, header, pred) for v in node
            ]
        if not isinstance(node, str):
            return node

        def field_of(m):
            field = m.group(1)
            if not _CATALOG_IDENT.match(field):
                self._catalog_fail(
                    where, "invalid_placeholder", context={"placeholder": m.group(0)}
                )
            if field not in header:
                _fail(
                    where,
                    "validation",
                    "unknown_row_catalog_column",
                    "REQ-1249",
                    {"field": field},
                )
            return field

        def literal(field):
            v = rec[field]
            if types.get(field, "str") in ("int", "float"):
                return str(v)
            return "'" + str(v).replace("'", "''") + "'"

        m = _CATALOG_PLACEHOLDER.fullmatch(node)
        if m:
            field = field_of(m)
            return literal(field) if pred else rec[field]

        def repl(mm):
            field = field_of(mm)
            if not pred:
                self._catalog_fail(
                    where, "embedded_placeholder", context={"placeholder": mm.group(0)}
                )
            return literal(field)

        return _CATALOG_PLACEHOLDER.sub(repl, node)

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

        for c in s.get("columns") or []:
            if isinstance(c, dict):
                expand(c.get("derivation"), f"columns.{c.get('name')}.derivation")
        for t in s.get("rows") or []:
            if isinstance(t, dict):
                for dn, dd in (t.get("derivations") or {}).items():
                    expand(dd, f"rows.{t.get('id')}.derivations.{dn}")
        for i, im in enumerate(s.get("intermediates") or []):
            if isinstance(im, dict):
                for dn, dd in (im.get("derivations") or {}).items():
                    expand(dd, f"intermediates[{i}].derivations.{dn}")

    def _reject_parents(self, parents):
        """R017 failure surface for the clean-room (no composition)."""
        if isinstance(parents, str):
            parents = [parents]
        for p in parents:
            if (
                not isinstance(p, str)
                or "://" in p
                or p.startswith("file:")
                or os.path.isabs(p)
            ):
                _fail(
                    "parents",
                    "validation",
                    "invalid_parent_path",
                    "R017-15",
                    {"parent": p},
                )
        seen = [os.path.normpath(self.spec_path)]
        for p in parents:
            self._check_parent_chain(
                os.path.normpath(os.path.join(self.spec_dir, p)), seen
            )
        if "output" not in self.spec:
            inherited = []
            for p in parents:
                full = os.path.normpath(os.path.join(self.spec_dir, p))
                with open(full, "r", encoding="utf-8") as f:
                    doc = yaml.safe_load(f) or {}
                if isinstance(doc, dict) and doc.get("output"):
                    inherited = (doc["output"] or {}).get("columns") or []
                    break
            _fail(
                "output",
                "validation",
                "missing_entry_output",
                "R017-44",
                {"inherited_columns": inherited},
            )
        for c in self.spec.get("columns") or []:
            if isinstance(c, dict) and "type" in c and c["type"] is None:
                _fail(
                    f"columns.{c.get('name')}.type",
                    "validation",
                    "invalid_clear",
                    "R017-47",
                    {"field": "type"},
                )
        _fail(
            "parents",
            "validation",
            "invalid_clear",
            "R017-19",
            {"detail": "spec composition is not implemented in the clean-room"},
        )

    def _check_parent_chain(self, full, seen):
        if full in seen:
            _fail(
                "parents",
                "validation",
                "inheritance_cycle",
                "R017-42",
                {"reason": "parent_chain_returns_to_entry"},
            )
        seen.append(full)
        with open(full, "r", encoding="utf-8") as f:
            doc = yaml.safe_load(f) or {}
        if not isinstance(doc, dict) or doc.get("schema_version") != "1.0":
            _fail(
                "parents",
                "validation",
                "schema_version_mismatch",
                "R017-9",
                {"parent": full},
            )
        sub = doc.get("parents")
        if sub:
            if isinstance(sub, str):
                sub = [sub]
            for p in sub:
                self._check_parent_chain(
                    os.path.normpath(os.path.join(os.path.dirname(full), p)), seen
                )
        seen.pop()

    def _load_input(self, name, decl):
        if isinstance(decl, str):
            path, types = decl, {}
        else:
            path, types = decl["path"], decl.get("types") or {}
        _validate.check_resource_path(self, name, path)  # R021
        lower = path.lower()
        if lower.endswith(".csv"):
            profile = "csv"
        elif lower.endswith(".parquet"):
            profile = "parquet"
        else:
            _fail(
                f"input.{name}.path",
                "validation",
                "source_profile_unknown",
                "REQ-0852",
                {"dataset": name, "path": path},
            )
        full = os.path.normpath(os.path.join(self.spec_dir, path))
        if profile == "parquet":
            fields, records = read_parquet(
                full, types, spec_path=f"input.{name}", dataset=name, written_path=path
            )
        else:
            fields, records = read_csv(
                full, types, spec_path=f"input.{name}", dataset=name, written_path=path
            )
        for f in types:
            if f not in fields:
                _fail(
                    f"input.{name}.types.{f}",
                    "validation",
                    "unknown_field",
                    "R014-19",
                    {"dataset": name, "field": f},
                )
        return Table(name, full, types, fields, records)

    def run(self):
        self.rows = []
        self._origins = []
        self._recs = []
        self._derived = []
        self._lookups = {}
        self._eligible = {}
        self._self_marks = []
        self._row_phase = True
        self._build_rows()
        self._derive_columns()
        self._verify()
        return self._render()

    def _default_dataset(self):
        if self.spec.get("base"):
            return self.spec["base"]
        if len(self.inputs) == 1:
            return next(iter(self.inputs))
        _fail("base", "validation", "missing_required_field", "R001-33", {})

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
        if ds not in self.inputs:
            _fail(
                f"rows.{t['id']}.dataset", "validation", "unknown_field", "R002-27", {}
            )
        table = self.inputs[ds]
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

    def _rec_pred(self, text, record, ds, where):
        node, _ = _pred.parse(text, where)

        def resolve(name):
            parts = name.split(".")
            if len(parts) == 2 and parts[0] == ds:
                if parts[1] not in record:
                    _fail(
                        where, "validation", "unknown_field", "R002-27", {"name": name}
                    )
                return record[parts[1]]
            if len(parts) == 1 and parts[0] in record:
                return record[parts[0]]
            _fail(where, "validation", "unknown_field", "R004-32", {"name": name})

        return _pred.evaluate(node, resolve, where)

    def _template_phases(self, t, derivs):
        """Split a template's derivations into phase A (non-window, not"""
        where = f"rows.{t['id']}.derivations"
        order = _validate._topo_order(derivs, _validate._unqualified_refs, where)
        nodes = {n: _norm_derivation(derivs[n], f"{where}.{n}") for n in order}
        winset = {
            n
            for n in order
            if isinstance(nodes[n], dict)
            and len(nodes[n]) == 1
            and next(iter(nodes[n])) in WINDOW_KEYS
        }
        closure = set(winset)
        changed = True
        while changed:
            changed = False
            for n in order:
                if n not in closure and _validate._unqualified_refs(nodes[n]) & closure:
                    closure.add(n)
                    changed = True
        phase_a = [n for n in order if n not in closure]
        phase_b = [n for n in order if n in winset]
        phase_c = [n for n in order if n in closure and n not in winset]
        return nodes, phase_a, phase_b, phase_c

    def _build_record_driven(self, t, table, filt, derivs):
        ds = table.name
        where = f"rows.{t['id']}.filter"
        recs = [
            rec
            for rec in table.records
            if filt is None or self._rec_pred(filt, rec, ds, where) is True
        ]
        nodes, phase_a, phase_b, phase_c = self._template_phases(t, derivs)
        if not phase_b:
            for rec in recs:
                ctx = _RowCtx(self, "row", t, ds, record=rec)
                row, derived = self._eval_row_derivs(t, derivs, ctx)
                self._append_row(row, ds, {ds: [rec]}, derived)
            return
        rows, ctxs = [], []
        for rec in recs:
            ctx = _RowCtx(self, "row", t, ds, record=rec)
            row = {}
            ctx.row = row
            for name in phase_a:
                ctx.where = f"rows.{t['id']}.derivations.{name}"
                v = _expr.eval_expr(nodes[name], ctx)
                if name in self.colspecs:
                    v = self._convert(v, self.colspecs[name], ctx.where)
                row[name] = v
            rows.append(row)
            ctxs.append(ctx)
        wctx = _RowWinCtx(self, t, ds, rows, recs)
        for name in phase_b:
            wctx.where = f"rows.{t['id']}.derivations.{name}"
            kind = next(iter(nodes[name]))
            vec = wctx.window_value(kind, nodes[name][kind])
            for i, v in enumerate(vec):
                rows[i][name] = v
        for row, ctx in zip(rows, ctxs):
            ctx.row = row
            for name in phase_c:
                ctx.where = f"rows.{t['id']}.derivations.{name}"
                v = _expr.eval_expr(nodes[name], ctx)
                if name in self.colspecs:
                    v = self._convert(v, self.colspecs[name], ctx.where)
                row[name] = v
        for row, rec in zip(rows, recs):
            self._append_row(row, ds, {ds: [rec]}, set(derivs))

    def _build_grouped(self, t, table, group_by, filt, derivs):
        ds = table.name
        gcols = [g.split(".")[-1] for g in group_by]
        for g in group_by:
            if "." not in g or g.split(".")[0] != ds:
                _fail(
                    f"rows.{t['id']}.group_by",
                    "validation",
                    "unknown_field",
                    "R001-35",
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
        nodes, phase_a, phase_b, phase_c = self._template_phases(t, derivs)
        if not phase_b:
            for _, keydict, grecords in groups:
                ctx = _RowCtx(self, "row", t, ds, group=grecords, groupkeys=keydict)
                row, derived = self._eval_row_derivs(t, derivs, ctx)
                if filt is not None:
                    node, _ = _pred.parse(filt, f"rows.{t['id']}.filter")
                    if (
                        _pred.evaluate(
                            node,
                            lambda n, row=row: row.get(n),
                            f"rows.{t['id']}.filter",
                        )
                        is not True
                    ):
                        continue
                self._append_row(row, ds, {ds: grecords}, derived)
            return
        pairs = []  # (row, ctx, grecords)
        for _, keydict, grecords in groups:
            ctx = _RowCtx(self, "row", t, ds, group=grecords, groupkeys=keydict)
            row = {}
            ctx.row = row
            for name in phase_a:
                ctx.where = f"rows.{t['id']}.derivations.{name}"
                v = _expr.eval_expr(nodes[name], ctx)
                if name in self.colspecs:
                    v = self._convert(v, self.colspecs[name], ctx.where)
                row[name] = v
            if filt is not None:
                node, _ = _pred.parse(filt, f"rows.{t['id']}.filter")
                if (
                    _pred.evaluate(
                        node,
                        lambda n, row=row: row.get(n),
                        f"rows.{t['id']}.filter",
                    )
                    is not True
                ):
                    continue
            pairs.append((row, ctx, grecords))
        rows = [p[0] for p in pairs]
        wctx = _RowWinCtx(self, t, ds, rows, None)
        for name in phase_b:
            wctx.where = f"rows.{t['id']}.derivations.{name}"
            kind = next(iter(nodes[name]))
            vec = wctx.window_value(kind, nodes[name][kind])
            for i, v in enumerate(vec):
                rows[i][name] = v
        for row, ctx, grecords in pairs:
            ctx.row = row
            for name in phase_c:
                ctx.where = f"rows.{t['id']}.derivations.{name}"
                v = _expr.eval_expr(nodes[name], ctx)
                if name in self.colspecs:
                    v = self._convert(v, self.colspecs[name], ctx.where)
                row[name] = v
            self._append_row(row, ds, {ds: grecords}, set(derivs))

    def _eval_row_derivs(self, t, derivs, ctx):
        order = _validate._topo_order(
            derivs, _validate._unqualified_refs, f"rows.{t['id']}.derivations"
        )
        row = {}
        ctx.row = row
        for name in order:
            ctx.where = f"rows.{t['id']}.derivations.{name}"
            node = _norm_derivation(derivs[name], ctx.where)
            v = _expr.eval_expr(node, ctx)
            cs = self.colspecs.get(name)
            if cs is None:
                row[name] = v
            else:
                row[name] = self._convert(v, cs, ctx.where, deriv=node)
        return row, set(derivs)

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
                _fail("keys", "validation", "unknown_field", "R005", {"key": k})
            d = cs.get("derivation")
            if d is None:
                _fail(
                    f"columns.{k}",
                    "validation",
                    "missing_required_field",
                    "R001-12",
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
                row[k] = self._convert(v, self.colspecs[k], ctx.where)
            krows.append(row)
            krecs.append(rec)
        for k in winkeys:
            node = keynodes[k]
            kind = next(iter(node))
            wctx = _KeyWinCtx(self, base, krows, krecs, f"columns.{k}.derivation")
            vec = wctx.window_value(kind, node[kind])
            for i, v in enumerate(vec):
                krows[i][k] = self._convert(
                    v, self.colspecs[k], f"columns.{k}.derivation"
                )
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
            self._recs[seen[tup]][base].append(krecs[i])
        self.rows = order

    def _append_row(self, row, origin, recs, derived):
        self.rows.append(row)
        self._origins.append(origin)
        self._recs.append(recs)
        self._derived.append(set(derived))

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
                v = _expr.eval_expr(node, ctx)
                self.rows[i][name] = self._convert(v, cs, ctx.where)
                self._derived[i].add(name)
        self._verify_self_uniques()

    def _derive_window(self, name, cs, node, kind):
        payload = node[kind]
        ctx = _ColCtx(self, 0, f"columns.{name}.derivation")
        vec = ctx.window_value(kind, payload)
        for i, v in enumerate(vec):
            if name in self._derived[i]:
                continue
            self.rows[i][name] = self._convert(v, cs, f"columns.{name}.derivation")
            self._derived[i].add(name)

    def _convert(self, v, cs, where, deriv=None):
        target = cs["type"]
        node = deriv if deriv is not None else cs.get("derivation")
        strict = isinstance(node, dict) and node.get("strict", False) is True
        has_missing = isinstance(node, dict) and "missing" in node
        try:
            return _convert_value(v, target)
        except _ConvertFail as cf:
            if strict or not has_missing:
                raise YamaaError(
                    phase="convert",
                    condition="conversion_failed",
                    requirement=cf.requirement,
                    spec_paths=[where],
                    context={"column": cs["name"], "type": target},
                )
            try:
                return _convert_value(node["missing"], target)
            except _ConvertFail as mf:
                raise YamaaError(
                    phase="convert",
                    condition="conversion_failed",
                    requirement=mf.requirement,
                    spec_paths=[where],
                    context={"column": cs["name"], "type": target},
                )

    def _verify(self):
        seen_ids = set()
        for v in self.verifications:
            self._verify_one(v, "verifications", None, seen_ids)
        for name in self.col_order:
            for v in _vlist(self.colspecs[name].get("verifications")):
                self._verify_one(v, f"columns.{name}.verifications", name, seen_ids)

    def _verify_self_uniques(self):
        """REQ-0120/1245: uniqueness over the current completed donor pool."""
        pool = (
            self.rows[: self._self_marks[-1]]
            if self._row_phase and self._self_marks
            else self.rows
        )
        for lid, decl in self.lookups_decl.items():
            if decl.get("dataset") != "SELF":
                continue
            uniq = (decl.get("verification") or {}).get("unique")
            if not uniq:
                continue
            if isinstance(uniq, str):
                uniq = [uniq]
            seen = set()
            for r in pool:
                tup = tuple(_hashable(r.get(u)) for u in uniq)
                if tup in seen:
                    _fail(
                        f"intermediates.{lid}.verification",
                        "verification",
                        "duplicate_intermediate_records",
                        "REQ-1245",
                        {"intermediate": lid, "unique": uniq},
                    )
                seen.add(tup)

    def _verify_one(self, v, where, col, seen_ids):
        if not isinstance(v, dict) or len(v) != 1:
            _fail(where, "validation", "invalid_field_type", "R009", {})
        kind, payload = next(iter(v.items()))
        payload = payload or {}
        if not isinstance(payload, dict):
            _fail(where, "validation", "invalid_field_type", "R009", {})
        vid = payload.get("id")
        if vid is not None:
            if vid in seen_ids:
                _fail(where, "validation", "duplicate_identifier", "R009", {"id": vid})
            seen_ids.add(vid)
        if kind in ("all_or_none", "implies", "assert") and vid is None:
            _fail(
                where,
                "validation",
                "missing_verification_id",
                "R009",
                {"verification": kind},
            )
        if kind == "row_count" and payload.get("group_by") and vid is None:
            _fail(
                where,
                "validation",
                "missing_verification_id",
                "R009-21",
                {"verification": kind},
            )
        severity = payload.get("severity", "error")
        failed = self._check_verification(kind, payload, col, where)
        if failed and severity == "error":
            _fail(where, "verification", failed, "R009", {"verification": kind})

    def _check_verification(self, kind, payload, col, where):
        if kind == "row_count":
            return self._check_row_count(payload, where)
        if kind == "unique":
            cols = payload["columns"]
            seen = set()
            for row in self.rows:
                tup = tuple(_hashable(row.get(c)) for c in cols)
                if tup in seen:
                    return "unique_failed"
                seen.add(tup)
            return None
        if kind == "all_or_none":
            for row in self.rows:
                vals = [row.get(c) for c in payload["columns"]]
                if any(is_missing(x) for x in vals) and not all(
                    is_missing(x) for x in vals
                ):
                    return "all_or_none_failed"
            return None
        if kind == "assert":
            node, _ = _pred.parse(payload["expr"], where)
            for i, row in enumerate(self.rows):
                ctx = _ColCtx(self, i, where)
                if _pred.evaluate(node, ctx.value, where) is not True:
                    return "assert_failed"
            return None
        if kind == "implies":
            wn, _ = _pred.parse(payload["when"], where)
            tn, _ = _pred.parse(payload["then"], where)
            for i, row in enumerate(self.rows):
                ctx = _ColCtx(self, i, where)
                w = _pred.evaluate(wn, ctx.value, where)
                if w is True and _pred.evaluate(tn, ctx.value, where) is not True:
                    return "implication_failed"
            return None
        col = col or payload.get("column")
        if col is None or col not in self.colspecs:
            _fail(where, "validation", "unknown_field", "R009", {"verification": kind})
        if kind == "not_missing":
            if any(is_missing(r.get(col)) for r in self.rows):
                return "not_missing_failed"
            return None
        if kind == "allowed_values":
            allowed = set(payload["values"])
            for r in self.rows:
                v = r.get(col)
                if not is_missing(v) and v not in allowed:
                    return "allowed_values_failed"
            return None
        if kind == "range":
            for r in self.rows:
                v = r.get(col)
                if is_missing(v):
                    continue
                if "min" in payload and compare(v, payload["min"]) < 0:
                    return "range_failed"
                if "max" in payload and compare(v, payload["max"]) > 0:
                    return "range_failed"
            return None
        if kind == "max_length":
            for r in self.rows:
                v = r.get(col)
                if not is_missing(v) and len(v) > payload["max"]:
                    return "length_failed"
            return None
        if kind == "matches":
            rx = _validate._normalize_pattern(payload["pattern"])
            for r in self.rows:
                v = r.get(col)
                if not is_missing(v) and not rx.search(v):
                    return "matches_failed"
            return None
        _fail(
            "verifications",
            "validation",
            "unknown_field",
            "R009",
            {"verification": kind},
        )

    def _check_row_count(self, payload, where):
        filt = payload.get("filter")
        group_by = payload.get("group_by")
        if filt is not None:
            node, _ = _pred.parse(filt, where)
            idxs = [
                i
                for i in range(len(self.rows))
                if _pred.evaluate(node, lambda n, i=i: self.rows[i].get(n), where)
                is True
            ]
        else:
            idxs = list(range(len(self.rows)))
        groups = [idxs]
        if group_by:
            parts = {}
            for i in idxs:
                tup = tuple(_hashable(self.rows[i].get(g)) for g in group_by)
                parts.setdefault(tup, []).append(i)
            groups = list(parts.values())
        for g in groups:
            n = len(g)
            if "min" in payload and payload["min"] is not None and n < payload["min"]:
                return "row_count_failed"
            if "max" in payload and payload["max"] is not None and n > payload["max"]:
                return "row_count_failed"
        return None

    def _render(self):
        seen = set()
        for i, row in enumerate(self.rows):
            tup = tuple(_hashable(row.get(k)) for k in self.keys)
            if any(is_missing(row.get(k)) for k in self.keys):
                _fail(
                    "output",
                    "output",
                    "missing_key",
                    "R005",
                    {"row": i, "keys": self.keys},
                )
            if tup in seen:
                _fail(
                    "output",
                    "output",
                    "duplicate_key",
                    "R005-51",
                    {"row": i, "keys": self.keys},
                )
            seen.add(tup)
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
                    "R005",
                    {"column": c},
                )
        ctypes = {c: self.colspecs[c]["type"] for c in cols}
        out_rows = [{c: r.get(c) for c in cols} for r in rows]
        return write_csv_text(
            cols, out_rows, ctypes, decimals=self.output.get("decimals")
        )


_ABSENT = _expr._ABSENT


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
    """R006-25/R007-57: bare string -> {source: s}; {value:} handled wrapper."""
    if isinstance(d, str):
        return {"source": d}
    if isinstance(d, dict) and len(d) == 1 and "value" in d:
        return d
    if isinstance(d, dict) and len(d) == 1:
        return d
    if (
        isinstance(d, dict)
        and "value" in d
        and set(d) <= {"value", "missing", "strict"}
    ):
        return d
    _fail(where, "validation", "invalid_field_type", "R007", {"derivation": d})


def _eval_intermediate_derivs(e, ds, recs, derivs, where):
    """REQ-1185: derivations in declaration order; windows compute over the
    full donor set as augmented by earlier derivations."""
    augmented = [dict(r) for r in recs]
    for dname, dnode in derivs.items():
        node = _norm_derivation(dnode, where)
        kind = next(iter(node)) if isinstance(node, dict) and len(node) == 1 else None
        if kind in _validate._WINDOW_KINDS:
            wctx = _DonorWinCtx(e, ds, augmented, where)
            vec = wctx.window_value(kind, node[kind])
            for rr, v in zip(augmented, vec):
                rr[dname] = v
        else:
            for rr in augmented:
                dctx = _DeriveCtx(e, ds, rr, rr, where, -1)
                rr[dname] = _expr.eval_expr(node, dctx)
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

    def record_predicate(self, text, record, default_ds=None):
        node, _ = _pred.parse(text, self.where)
        ds = record.get("_ds") or default_ds

        def resolve(name):
            parts = name.split(".")
            if len(parts) == 2 and parts[0] == ds:
                return record.get(parts[1])
            if len(parts) == 1:
                return record.get(name)
            _fail(self.where, "validation", "unknown_field", "R004-32", {"name": name})

        return _pred.evaluate(node, resolve, self.where)

    def order_records(self, recs, order_by):
        return _order_records(recs, order_by, self.where)

    def inline_lookup(self, payload):
        sig = json.dumps(
            {
                f: payload.get(f)
                for f in (
                    "key",
                    "key_base",
                    "between",
                    "filter",
                    "order_by",
                    "keep",
                    "strict",
                )
            },
            sort_keys=True,
            default=str,
        )
        decl = {
            "id": f"<inline:{payload['dataset']}:{payload['value']}:{sig}>",
            "dataset": payload["dataset"],
            "key": payload.get("key"),
            "key_base": payload.get("key_base"),
            "between": payload.get("between"),
            "filter": payload.get("filter"),
            "order_by": payload.get("order_by"),
            "keep": payload.get("keep"),
            "columns": None,
            "missing": payload.get("missing"),
            "strict": payload.get("strict", False),
        }
        rec = self._match_lookup(decl, self._row_index())
        if rec is None:
            return decl["missing"]
        col = payload["value"]
        if col not in rec:
            _fail(self.where, "validation", "unknown_field", "R003-15", {"column": col})
        return rec[col]

    def _row_index(self):
        raise NotImplementedError

    def _driver_ds(self):
        """REQ-0120: the driver dataset is the root base, or the sole input
        when no base is declared."""
        base = self.e.spec.get("base")
        if base is not None:
            return base
        ins = list(self.e.inputs)
        return ins[0] if len(ins) == 1 else None

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
                recs = _eval_intermediate_derivs(
                    self.e, ds, recs, decl["derivations"], self.where
                )
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
                        "R003-22",
                        {"name": name},
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
            uniq = (decl.get("verification") or {}).get("unique")
            if uniq:
                seen = set()
                for r in recs:
                    tup = tuple(_hashable(r.get(u)) for u in uniq)
                    if tup in seen:
                        _fail(
                            self.where,
                            "verification",
                            "duplicate_intermediate_records",
                            "REQ-1245",
                            {"intermediate": decl["id"], "unique": uniq},
                        )
                    seen.add(tup)
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
            donors = _eval_intermediate_derivs(
                self.e, "SELF", donors, decl["derivations"], self.where
            )
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
        _fail(self.where, "validation", "unknown_field", "R003-22", {"name": name})

    def _lookup_keys(self, decl):
        if decl["dataset"] == "SELF":
            fields = set(getattr(self.e, "donor_fields", None) or set())
            ds_label = "SELF"
        else:
            table = self.e.inputs[decl["dataset"]]
            fields = set(table.fields)
            ds_label = decl["dataset"]
        key = decl.get("key")
        if key is None:
            key = [k for k in self.e.keys if k in fields]
            if not key:
                _fail(
                    self.where,
                    "validation",
                    "no_applicable_keys",
                    "R003-43",
                    {"dataset": ds_label},
                )
        elif isinstance(key, str):
            key = [key]
        key_base = decl.get("key_base")
        if key_base is None:
            key_base = list(key)
        elif isinstance(key_base, str):
            key_base = [key_base]
        if len(key) != len(key_base) or not key:
            _fail(self.where, "validation", "source_key_length_mismatch", "R003-5", {})
        for k in key:
            if k not in fields:
                _fail(self.where, "validation", "unknown_field", "R003-6", {"key": k})
        return key, key_base

    def _match_lookup(self, decl, i):
        cache_key = (decl["id"], i)
        if cache_key in self.e._lookups:
            return self.e._lookups[cache_key]
        recs = self._eligible(decl)
        key, key_base = self._lookup_keys(decl)
        match_vals = [self._match_value(v, i) for v in key_base]
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
                cands = self._apply_between(cands, decl["between"], i)
            corr = self._correlated_filter(decl)
            if corr is not None:
                node, driver, donor = corr
                drec = None
                if 0 <= i < len(self.e._recs):
                    ds_recs = (self.e._recs[i] or {}).get(driver, [])
                    drec = ds_recs[0] if ds_recs else None

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
                        {"name": name},
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
                    _fail(
                        self.where,
                        "join",
                        "multiple_matches",
                        "R003-17",
                        {"lookup": decl["id"]},
                    )
                cands = _order_records(cands, ob, self.where)
                cands = [cands[0] if keep == "first" else cands[-1]]
        if not cands:
            if decl.get("strict"):
                _fail(
                    self.where,
                    "join",
                    "unmatched_key",
                    "R003-14",
                    {"lookup": decl["id"], "key": key},
                )
            self.e._lookups[cache_key] = None
            return None
        if decl.get("strict") and decl.get("missing") is not None:
            _fail(self.where, "validation", "conflicting_absent_policy", "R003-13", {})
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

    def _apply_between(self, cands, between, i):
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
                continue  # R003-18: missing bound -> ineligible
            if not comparable(val, lo or hi or val):
                _fail(
                    self.where, "validation", "incomparable_range_types", "R003-11", {}
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
            return decl.get("missing")
        cols = decl.get("columns")
        if cols is not None and col not in cols:
            _fail(self.where, "validation", "unknown_field", "R003-15", {"column": col})
        if col not in rec:
            _fail(self.where, "validation", "unknown_field", "R003-15", {"column": col})
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
            vals = self._window_kind(kind, payload, ordered, e)
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
                        self.where,
                        "derivation",
                        "incompatible_input_type",
                        "R016-38",
                        {},
                    )
                if compare(d, r) <= 0:
                    cands.append((d, i))
            out = [None] * len(ordered)
            if cands:
                latest = max(c for c, _ in cands)
                winners = [i for c, i in cands if compare(c, latest) == 0]
                if len(winners) > 1:
                    _fail(self.where, "derivation", "ambiguous_baseline", "R007", {})
                out[ordered.index(winners[0])] = "Y"
            return out
        _fail(self.where, "validation", "unknown_field", "R007", {"window": kind})

    def aggregate_value(self, payload):
        if isinstance(payload, str):
            payload = {"expr": payload}
        if payload.get("derive"):
            try:
                return self._agg_derive(payload)
            except YamaaError as e:
                raise _only_error(e, self.where) from e
        node, names = _agg.parse(payload["expr"], self.where)
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
                "R013-39",
                {"expr": payload["expr"]},
            )
        if stars and not qualified:
            if len(stars) > 1:
                _fail(
                    self.where,
                    "validation",
                    "prohibited_construct",
                    "R013-39",
                    {"expr": payload["expr"]},
                )
            try:
                return self._agg_qualified(node, payload, sorted(stars))
            except YamaaError as e:
                raise _only_error(e, self.where) from e
        if qualified:
            try:
                return self._agg_qualified(node, payload, qualified)
            except YamaaError as e:
                raise _only_error(e, self.where) from e
        try:
            return self._agg_unqualified(node, payload, plain, i)
        except YamaaError as e:
            raise _only_error(e, self.where) from e

    def _agg_derive(self, payload):
        """Reduce over per-record derive bindings (REQ-1189/1190)."""
        e = self.e
        node, _names = _agg.parse(payload["expr"], self.where)
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
        key_base = payload.get("key_base") or list(key)
        if isinstance(key, str):
            key = [key]
        if isinstance(key_base, str):
            key_base = [key_base]
        if len(key) != len(key_base):
            _fail(self.where, "validation", "source_key_length_mismatch", "R003-5", {})
        i = self._row_index()
        base_vals = [self._match_value(kb, i) for kb in key_base]
        matched = []
        if not any(is_missing(v) for v in base_vals):
            table = e.inputs[ds]
            matched = [
                r
                for r in table.records
                if all(
                    _hashable(r.get(kf)) == _hashable(bv)
                    for kf, bv in zip(key, base_vals)
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
            for b in derive:
                bnode = _norm_derivation(b["derivation"], self.where)
                v = _expr.eval_expr(bnode, dctx)
                v = self.e._convert(
                    v, {"name": b["name"], "type": b["type"]}, self.where, deriv=bnode
                )
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
                "R013-39",
                {"expr": payload["expr"]},
            )
        table = e.inputs.get(ds)
        if table is None:
            _fail(self.where, "validation", "unknown_field", "R002-27", {"dataset": ds})
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
                        "R013-42",
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
            kb = payload.get("key_base") or list(key)
            if isinstance(kb, str):
                kb = [kb]
            mvals = [self._match_value(v, i) for v in kb]
            if any(is_missing(v) for v in mvals):
                return None
            matched = [
                r
                for r in recs
                if all(
                    not is_missing(r.get(k))
                    and comparable(mv, r.get(k))
                    and compare(mv, r.get(k)) == 0
                    for k, mv in zip(key, mvals)
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
                "R013-42",
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
        self._col_phase = True  # R001-44 applies to column derivations

    def _row_index(self):
        return self.i

    def _row_value(self, i, name):
        return self.e.rows[i].get(name)

    def value(self, name):
        e = self.e
        if "." in name:
            parts = name.split(".")
            if parts[0] in e.lookups_decl:
                return self.lookup_value(parts[0], parts[1])
            recs = self.source_records(name)
            field = self.value_field(name)
            vals = [r.get(field) for r in recs]
            present = [v for v in vals if not is_missing(v)]
            seen_h, distinct = set(), []
            for v in present:
                h = (
                    ("num", float(v))
                    if isinstance(v, (int, float)) and not isinstance(v, bool)
                    else _hashable(v)
                )
                if h not in seen_h:
                    seen_h.add(h)
                    distinct.append(v)
            if len(distinct) > 1:
                _fail(
                    self.where,
                    "derivation",
                    "multiple_values_per_key",
                    "R001-44",
                    {
                        "identifier": name,
                        "value_count": len(distinct),
                        "keys": [{k: e.rows[self.i].get(k) for k in e.keys}],
                    },
                )
            if not recs:
                return None
            return distinct[0] if distinct else None
        row = e.rows[self.i]
        if name not in row:
            origin = e._origins[self.i]
            table = e.inputs.get(origin) if origin else None
            if table is not None and name in table.fields:
                _fail(
                    self.where,
                    "validation",
                    "unresolvable_name",
                    "REQ-0189",
                    {"identifier": name, "suggestion": f"{origin}.{name}"},
                )
            _fail(self.where, "validation", "unknown_field", "R001-39", {"name": name})
        return row[name]

    def source_records(self, var):
        e = self.e
        parts = var.split(".")
        ds = parts[0]
        i = self.i
        if ds in e.lookups_decl:
            rec = self._match_lookup(e.lookups_decl[ds], i)
            return [rec] if rec is not None else []
        table = e.inputs.get(ds)
        if table is None:
            _fail(
                self.where, "validation", "unknown_field", "R002-27", {"variable": var}
            )
        field = parts[-1]
        if field not in table.fields:
            _fail(
                self.where, "validation", "unknown_field", "R002-27", {"variable": var}
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
                "R003-42",
                {"variable": var},
            )
        row = e.rows[i]
        out = []
        for r in table.records:
            ok = True
            for k in applicable:
                mv, rv = row.get(k), r.get(k)
                if (
                    is_missing(mv)
                    or is_missing(rv)
                    or not comparable(mv, rv)
                    or compare(mv, rv) != 0
                ):
                    ok = False
                    break
            if ok:
                out.append(r)
        return out

    def compute_ident(self, name):
        e = self.e
        if "." in name:
            parts = name.split(".")
            if parts[0] in e.lookups_decl:
                return self.lookup_value(parts[0], parts[1])
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "R010-38",
                {"identifier": name},
            )
        return self.value(name)


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
        _fail(self.where, "row_construction", "phase_boundary", "R003-16", {})

    def _row_value(self, i, name):
        _fail(self.where, "row_construction", "phase_boundary", "R003-16", {})

    def value(self, name):
        if "." in name:
            parts = name.split(".")
            if parts[0] in self.e.lookups_decl:
                decl = self.e.lookups_decl[parts[0]]
                key, key_base = self._lookup_keys(decl)
                match_vals = [self._row_match_value(v) for v in key_base]
                rec = self._match_lookup_row(decl, key, match_vals)
                if rec is None:
                    return decl.get("missing")
                col = parts[1]
                cols = decl.get("columns")
                if (cols is not None and col not in cols) or col not in rec:
                    _fail(
                        self.where,
                        "validation",
                        "unknown_field",
                        "R003-15",
                        {"column": col},
                    )
                return rec.get(col)
            recs = self.source_records(name)
            if len(recs) == 1:
                return recs[0].get(self.value_field(name))
            if not recs:
                return None
            _fail(
                self.where,
                "row_construction",
                "multiple_matches",
                "R003-17",
                {"variable": name},
            )
        if name in self.row:
            return self.row[name]
        if name in self.groupkeys:
            return self.groupkeys[name]
        table = self.e.inputs.get(self.ds)
        if table is not None and name in table.fields:
            _fail(
                self.where,
                "validation",
                "unresolvable_name",
                "REQ-0189",
                {"identifier": name, "suggestion": f"{self.ds}.{name}"},
            )
        _fail(self.where, "validation", "unknown_field", "R001-39", {"name": name})

    def source_records(self, var):
        parts = var.split(".")
        ds = parts[0]
        if ds in self.e.lookups_decl:
            decl = self.e.lookups_decl[ds]
            key, key_base = self._lookup_keys(decl)
            match_vals = [self._row_match_value(v) for v in key_base]
            rec = self._match_lookup_row(decl, key, match_vals)
            return [rec] if rec is not None else []
        table = self.e.inputs.get(ds)
        if table is None:
            _fail(
                self.where, "validation", "unknown_field", "R002-27", {"variable": var}
            )
        if len(parts) >= 3 and parts[1] not in table.fields:
            _fail(
                self.where, "validation", "unknown_field", "R002-27", {"variable": var}
            )
        if ds == self.ds:
            if self.record is not None:
                return [self.record]
            if parts[1] in self.groupkeys:
                return [{parts[1]: self.groupkeys[parts[1]]}]
            _fail(
                self.where,
                "row_construction",
                "ungrouped_driver_field",
                "R001-36",
                {"variable": var},
            )
        applicable = [k for k in self.e.keys if k in table.fields]
        if not applicable:
            _fail(
                self.where,
                "validation",
                "no_applicable_keys",
                "R003-42",
                {"variable": var},
            )
        anchor = self.record if self.record is not None else self.groupkeys
        out = []
        for r in table.records:
            ok = True
            for k in applicable:
                mv = anchor.get(k)
                rv = r.get(k)
                if self.group is not None and k not in self.groupkeys:
                    _fail(
                        self.where,
                        "row_construction",
                        "ungrouped_driver_field",
                        "R001-36",
                        {"key": k},
                    )
                if (
                    is_missing(mv)
                    or is_missing(rv)
                    or not comparable(mv, rv)
                    or compare(mv, rv) != 0
                ):
                    ok = False
                    break
            if ok:
                out.append(r)
        return out

    def record_predicate(self, text, record, default_ds=None):
        record = dict(record)
        record["_ds"] = self.ds
        return super().record_predicate(text, record)

    def order_records(self, recs, order_by):
        return _order_records(recs, order_by, self.where)

    def inline_lookup(self, payload):
        ds = payload.get("dataset")
        if ds not in self.e.inputs:
            _fail(
                self.where,
                "row_construction",
                "phase_boundary",
                "R003-16",
                {"lookup": ds},
            )
        sig = json.dumps(
            {
                f: payload.get(f)
                for f in (
                    "key",
                    "key_base",
                    "between",
                    "filter",
                    "order_by",
                    "keep",
                    "strict",
                )
            },
            sort_keys=True,
            default=str,
        )
        decl = {
            "id": f"<inline:{ds}:{payload.get('value')}:{sig}>",
            "dataset": ds,
            "key": payload.get("key"),
            "key_base": payload.get("key_base"),
            "between": payload.get("between"),
            "filter": payload.get("filter"),
            "order_by": payload.get("order_by"),
            "keep": payload.get("keep"),
            "columns": None,
            "missing": payload.get("missing"),
            "strict": payload.get("strict", False),
        }
        key, key_base = self._lookup_keys(decl)
        match_vals = [self._row_match_value(v) for v in key_base]
        rec = self._match_lookup_row(decl, key, match_vals)
        if rec is None:
            return decl["missing"]
        col = payload.get("value")
        if col not in rec:
            _fail(self.where, "validation", "unknown_field", "R003-15", {"column": col})
        return rec[col]

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
                            "R002-27",
                            {"variable": var},
                        )
                    return self.record.get(field)
                if field in self.groupkeys:
                    return self.groupkeys[field]
            _fail(
                self.where,
                "row_construction",
                "phase_boundary",
                "R003-16",
                {"variable": var},
            )
        if var in self.row:
            return self.row[var]
        if var in self.groupkeys:
            return self.groupkeys[var]
        if var in ((self.template or {}).get("derivations") or {}):
            _fail(
                self.where,
                "row_construction",
                "phase_boundary",
                "R003-16",
                {"variable": var},
            )
        if self.record is not None and var in self.record:
            return self.record[var]
        _fail(self.where, "validation", "unknown_field", "R001-39", {"name": var})

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
                cands = self._apply_between(cands, decl["between"], -1)
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
                        {"name": name},
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
                    _fail(
                        self.where,
                        "join",
                        "multiple_matches",
                        "R003-17",
                        {"lookup": decl["id"]},
                    )
                cands = _order_records(cands, ob, self.where)
                cands = [cands[0] if keep == "first" else cands[-1]]
        if not cands:
            if decl.get("strict"):
                _fail(
                    self.where,
                    "join",
                    "unmatched_key",
                    "R003-14",
                    {"lookup": decl["id"], "key": key},
                )
            self.e._lookups[ck] = None
            return None
        if decl.get("strict") and decl.get("missing") is not None:
            _fail(self.where, "validation", "conflicting_absent_policy", "R003-13", {})
        self.e._lookups[ck] = cands[0]
        return cands[0]

    def _match_value(self, var, i):
        return self._row_match_value(var)

    def window_value(self, kind, payload):
        _fail(self.where, "row_construction", "phase_boundary", "R007", {})

    def aggregate_value(self, payload):
        if self.group is None:
            _fail(self.where, "row_construction", "prohibited_construct", "R013-43", {})
        if isinstance(payload, str):
            payload = {"expr": payload}
        node, names = _agg.parse(payload["expr"], self.where)
        ds = self.ds
        for n in names:
            if "." in n and n.split(".")[0] != ds:
                _fail(
                    self.where,
                    "validation",
                    "prohibited_construct",
                    "R013-43",
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
            if (
                e.phase == "row_construction"
                and e.condition == "multiple_values_per_key"
            ):
                raise YamaaError(
                    phase="row_construction",
                    condition="aggregate_multiple_records",
                    requirement="REQ-0501",
                    spec_paths=[self.where],
                    context={
                        "reducer": "ONLY",
                        "row": (self.template or {}).get("id"),
                        "group": dict(self.groupkeys),
                        "record_count": len(recs),
                    },
                )
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
                    "row_construction",
                    "ungrouped_driver_field",
                    "R001-36",
                    {"identifier": name},
                )
            _fail(
                self.where,
                "validation",
                "prohibited_construct",
                "R010-38",
                {"identifier": name},
            )
        return self.value(name)


class _RowWinCtx(_BaseCtx):
    """Row-template window phase (REQ-0326): windows over one template's"""

    def __init__(self, engine, template, ds, rows, recs):
        super().__init__(engine, f"rows.{template['id']}.derivations")
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
                    "row_construction",
                    "ungrouped_driver_field",
                    "R001-36",
                    {"identifier": var},
                )
            _fail(
                self.where,
                "validation",
                "unknown_field",
                "R001-39",
                {"identifier": var},
            )
        if var in self.trows[i]:
            return self.trows[i][var]
        _fail(self.where, "validation", "unknown_field", "R001-39", {"identifier": var})


class _KeyCtx(_BaseCtx):
    """Key-table derivation: per input record, base-qualified sources only."""

    def __init__(self, engine, base, record):
        super().__init__(engine, "<key>")
        self.base = base
        self.record = record

    def _row_index(self):
        _fail(self.where, "row_construction", "phase_boundary", "R001-38", {})

    def _row_value(self, i, name):
        _fail(self.where, "row_construction", "phase_boundary", "R001-38", {})

    def value(self, name):
        parts = name.split(".")
        if len(parts) == 2 and parts[0] == self.base:
            return self.record.get(parts[1])
        _fail(self.where, "validation", "forward_reference", "R001-43", {"name": name})

    def source_records(self, var):
        parts = var.split(".")
        if len(parts) == 2 and parts[0] == self.base:
            return [self.record]
        _fail(
            self.where, "validation", "forward_reference", "R001-43", {"variable": var}
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
            "R001-43",
            {"identifier": name},
        )

    def inline_lookup(self, payload):
        _fail(self.where, "validation", "forward_reference", "R001-43", {})

    def window_value(self, kind, payload):
        _fail(self.where, "validation", "forward_reference", "R001-43", {})

    def aggregate_value(self, payload):
        _fail(self.where, "validation", "forward_reference", "R001-43", {})


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
                "R001-43",
                {"identifier": var},
            )
        if var in self.krows[i]:
            return self.krows[i][var]
        _fail(
            self.where,
            "validation",
            "forward_reference",
            "R001-43",
            {"identifier": var},
        )

    def value(self, name):
        _fail(self.where, "validation", "forward_reference", "R001-43", {"name": name})

    def source_records(self, var):
        _fail(
            self.where, "validation", "forward_reference", "R001-43", {"variable": var}
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
                    "R002-27",
                    {"variable": name},
                )
            if f not in self.record:
                _fail(
                    self.where,
                    "validation",
                    "unknown_field",
                    "R002-27",
                    {"variable": name},
                )
            return self.record[f]
        if name in self.scope:
            return self.scope[name]
        _fail(self.where, "validation", "unknown_field", "R002", {"identifier": name})

    def source_records(self, var):
        d, _, _ = var.partition(".")
        if d != self.ds:
            if var in self.inter:
                return [{var.split(".")[-1]: self.inter[var]}]
            _fail(
                self.where, "validation", "unknown_field", "R002-27", {"variable": var}
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
            _fail(self.where, "validation", "unknown_field", "R004-32", {"name": name})

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
                    "R002-27",
                    {"variable": var},
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
                "derivation",
                "incompatible_input_type",
                "R007-38",
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
