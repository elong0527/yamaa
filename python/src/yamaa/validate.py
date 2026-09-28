"""Stage-1 shape validation for the clean-room engine."""

import os
import re

from . import agg as _agg
from . import expr as _expr
from . import numeric as _numeric
from . import pred as _pred
from .errors import YamaaError
from .values import YDate, YDateTime

COLUMN_TYPES = {"str", "int", "float", "date", "datetime"}

_VARIABLE_FIELDS = {
    "cut": ("source",),
    "str_extract": ("source",),
    "str_case": ("source",),
    "str_pad": ("source",),
    "date_diff": ("start", "end"),
    "date_impute": ("source", "not_before"),
    "date_precision": ("source",),
    "to_date": ("source",),
    "study_day": ("date", "reference"),
    "round_half_away_from_zero": ("source",),
    "function": ("name", "contract_version"),
    "row_value": ("source",),
    "previous_non_missing": ("source",),
    "locf": ("source",),
    "baseline_flag": ("date", "reference_date"),
}

_WINDOW_KINDS = {
    "row_number",
    "rank",
    "row_value",
    "previous_non_missing",
    "locf",
    "baseline_flag",
}


# REQ-1048: the intermediate_class fields.
_INTERMEDIATE_FIELDS = {
    "id",
    "dataset",
    "key",
    "between",
    "filter",
    "order_by",
    "keep",
    "columns",
    "no_match",
    "derivations",
    "verifications",
}


def _fail(where, phase, condition, requirement=None, context=None):
    raise YamaaError(
        phase=phase,
        condition=condition,
        requirement=requirement,
        spec_paths=[where] if isinstance(where, str) else where,
        context=context or {},
    )


def _type_name(v):
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "bool"
    if isinstance(v, int):
        return "int"
    if isinstance(v, float):
        return "float"
    if isinstance(v, str):
        return "str"
    if isinstance(v, list):
        return "list"
    if isinstance(v, dict) and len(v) == 1:
        return next(iter(v))
    return "object"


def _norm(d, where):
    if isinstance(d, str):
        return {"source": d}
    return d


def check_resource_path(e, name, path):
    where = f"input.{name}.path"
    ctx = {"dataset": name, "path": path}
    if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*://", path) or re.match(
        r"^[A-Za-z][A-Za-z0-9+.-]*:", path
    ):
        _fail(where, "validation", "resource_path_uri_scheme", "REQ-0775", ctx)
    if os.path.isabs(path):
        _fail(where, "validation", "resource_path_not_relative", "REQ-0781", ctx)
    full = os.path.normpath(os.path.join(e.spec_dir, path))
    if os.path.commonpath([e.spec_dir, full]) != e.spec_dir:
        _fail(where, "validation", "resource_path_outside_project", "REQ-0784", ctx)
    if os.path.islink(full):
        _fail(where, "validation", "resource_path_symlink", "REQ-0783", ctx)
    if not os.path.lexists(full):
        _fail(where, "validation", "resource_path_missing", "REQ-0785", ctx)
    if not os.path.isfile(full):
        _fail(where, "validation", "resource_path_not_regular_file", "REQ-0785", ctx)


def _check_portable_pattern(pattern, where):
    """REQ-0827/34: fail validation with invalid_regex when the portable"""
    if not isinstance(pattern, str):
        _fail(
            where,
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "str", "actual": _type_name(pattern)},
        )
    bad = _pred.portable_pattern_error(pattern)
    if bad is not None:
        _fail(
            where,
            "validation",
            "invalid_regex",
            "REQ-0827",
            {"pattern": pattern, "detail": bad},
        )


def _normalize_pattern(pattern):
    """Portable-regex normalization for Python's re (operations/text);
    shared definition in pred.py."""
    return _pred.normalize_pattern(pattern)


def run(e):
    _check_inputs(e)
    _check_columns(e)
    _check_output(e)
    _check_intermediates(e)
    _check_rows(e)
    _check_row_windows(e)
    _check_cycles(e)
    _check_derivations(e)
    _check_functions(e)
    _check_dependencies(e)
    _check_verifications(e)


def _check_inputs(e):
    s = e.spec
    for name, decl in (s.get("input", {}) or {}).items():
        if name == s.get("domain"):
            _fail(
                ["input." + name, "domain"],
                "validation",
                "duplicate_identifier",
                "REQ-0080",
                {"identifier": name},
            )
        if isinstance(decl, dict) and "schema" in decl:
            tps = decl.get("types") or {}
            if isinstance(tps, dict):
                for fname, ftype in tps.items():
                    _fail(
                        f"input.{name}.types.{fname}",
                        "validation",
                        "redundant_field_type",
                        "REQ-0523",
                        {"dataset": name, "field": fname, "type": ftype},
                    )


def _check_columns(e):
    for c in e.spec.get("columns", []) or []:
        t = c.get("type")
        if t not in COLUMN_TYPES:
            _fail(
                f"columns.{c['name']}.type",
                "validation",
                "value_not_permitted",
                "REQ-0012",
                {"value": t, "permitted": sorted(COLUMN_TYPES)},
            )


def _check_output(e):
    out = e.spec.get("output") or {}
    declared = set(e.col_order)
    for c in out.get("columns", []) or []:
        if c not in declared:
            _fail(
                "output.columns",
                "validation",
                "undeclared_column",
                "REQ-0234",
                {"column": c},
            )
    path = out.get("path")
    if isinstance(path, str):
        _, dot, ext = path.rpartition(".")
        if not dot or ("." + ext.lower()) not in (".csv", ".parquet"):
            _fail(
                "output.path",
                "validation",
                "unknown_artifact_profile",
                "REQ-1194",
                {"path": path},
            )
    for k in e.spec.get("keys", []) or []:
        if k not in set(out.get("columns", []) or []):
            idx = list(e.spec.get("keys", [])).index(k)
            _fail(
                f"keys[{idx}]",
                "validation",
                "internal_column_in_keys",
                "REQ-0233",
                {"column": k},
            )
    seen = set()
    for i, term in enumerate(out.get("order_by", []) or []):
        var = term if isinstance(term, str) else term.get("variable")
        if not isinstance(var, str) or var not in declared:
            _fail(
                f"output.order_by[{i}]",
                "validation",
                "undeclared_column",
                "REQ-0236",
                {"column": var},
            )
        if var in seen:
            _fail(
                f"output.order_by[{i}]",
                "validation",
                "duplicate_order_term",
                "REQ-0237",
                {"column": var},
            )
        seen.add(var)


def _safe_norm(d, where):
    """Normalize a derivation for promotion analysis; None when the
    derivation is malformed (the real validation reports it later)."""
    try:
        return _norm(d, where)
    except Exception:  # noqa: BLE001
        return None


def _safe_idents(node):
    """(bare, qualified) refs of a node; ([], []) when malformed."""
    try:
        return _ident_refs(node)
    except Exception:  # noqa: BLE001
        return [], []


def _node_has_phase_break(node):
    """True when a normalized derivation node uses a construct that cannot"""
    stack = [node]
    while stack:
        n = stack.pop()
        if isinstance(n, dict):
            if len(n) == 1:
                k = next(iter(n))
                if k == "aggregate" or k in _WINDOW_KINDS:
                    return True
            stack.extend(n.values())
        elif isinstance(n, list):
            stack.extend(n)
    return False


def _compute_row_defaults(e):
    """REQ-1260: every row-local column-level derivation is that column's"""
    col_nodes = {}
    for name in e.col_order:
        d = e.colspecs[name].get("derivation")
        if d is not None:
            n = _safe_norm(d, f"columns.{name}.derivation")
            if n is not None:
                col_nodes[name] = n
    inputs = set(e.inputs)
    row_local = {}

    def is_row_local(name, stack=()):
        if name in row_local:
            return row_local[name]
        if name in stack:
            return False
        node = col_nodes[name]
        ok = not _node_has_phase_break(node)
        if ok:
            bare, qual = _safe_idents(node)
            ok = all(q.split(".")[0] in inputs for q in qual) and all(
                b in col_nodes and is_row_local(b, stack + (name,)) for b in bare
            )
        row_local[name] = ok
        return ok

    e.row_defaults = {name for name in col_nodes if is_row_local(name)}
    e.donor_fields = set(e.col_order)


def _check_intermediates(e):
    _compute_row_defaults(e)
    inputs = set(e.inputs)
    seen = {}
    for i, d in enumerate(e.spec.get("intermediates", []) or []):
        lid = d.get("id")
        where = f"intermediates[{i}]"
        if lid in seen or lid in inputs:
            other = f"intermediates[{seen[lid]}].id" if lid in seen else f"input.{lid}"
            _fail(
                [where + ".id", other],
                "validation",
                "duplicate_identifier",
                "REQ-0113",
                {"identifier": lid},
            )
        seen[lid] = i
        # REQ-0285: intermediate_class is closed; the retired `key_base`,
        # `strict`, and `missing` are not its fields.
        for f in d:
            if f not in _INTERMEDIATE_FIELDS:
                _fail(
                    f"{where}.{f}",
                    "validation",
                    "unknown_field",
                    "REQ-0285",
                    {"intermediate": lid, "field": f},
                )
        ds = d.get("dataset")
        is_self = ds == "SELF"
        if is_self:
            if not (e.spec.get("rows") or []):
                _fail(
                    where,
                    "validation",
                    "prohibited_construct",
                    "REQ-0120",
                    {"dataset": ds},
                )
        elif ds not in e.inputs:
            _fail(where, "validation", "unknown_field", "REQ-0120", {"dataset": ds})
        # REQ-1248: `no_match: null` answers absence exactly as the implicit
        # join does, so it adds nothing to a bare alias.
        if set(d) <= {"id", "dataset"} or (
            set(d) == {"id", "dataset", "no_match"} and d["no_match"] is None
        ):
            _fail(
                where,
                "validation",
                "rename_only_intermediate",
                "REQ-1248",
                {"intermediate": lid, "dataset": ds},
            )
        key = d.get("key")
        if isinstance(key, str):
            key = [key]
        if key is not None:
            if isinstance(key, dict):
                # REQ-0115 mapping form: donor field -> driver match expression.
                driver_vals = list(key.values())
                for kf in key:
                    if not isinstance(kf, str):
                        _fail(
                            where + ".key",
                            "validation",
                            "invalid_field_type",
                            "REQ-0115",
                            {"expected": "field name", "actual": _type_name(kf)},
                        )
            elif isinstance(key, list):
                driver_vals = list(key)
                for kf in key:
                    if not isinstance(kf, str):
                        _fail(
                            where + ".key",
                            "validation",
                            "invalid_field_type",
                            "REQ-0115",
                            {"expected": "field name", "actual": _type_name(kf)},
                        )
            else:
                _fail(
                    where + ".key",
                    "validation",
                    "invalid_field_type",
                    "REQ-0115",
                    {"expected": "list or mapping", "actual": _type_name(key)},
                )
            # REQ-0117: plain-name match values must name known current-row values.
            for v in driver_vals:
                if isinstance(v, str):
                    if "." not in v and v not in e.donor_fields:
                        _fail(
                            where + ".key",
                            "validation",
                            "unknown_field",
                            "REQ-0117",
                            {"identifier": v, "intermediate": lid},
                        )
                elif isinstance(v, dict) and len(v) == 1:
                    _check_tree(e, v, where + ".key", "col")
                else:
                    _fail(
                        where + ".key",
                        "validation",
                        "invalid_field_type",
                        "REQ-0322",
                        {"expected": "variable or expression", "actual": _type_name(v)},
                    )
        ob, keep = d.get("order_by"), d.get("keep")
        if bool(ob) != bool(keep):
            _fail(
                where,
                "validation",
                "unpaired_fields",
                "REQ-0119",
                {
                    "intermediate": lid,
                    "declared": ["order_by"] if ob else ["keep"],
                    "missing": ["keep"] if ob else ["order_by"],
                },
            )
        filt = d.get("filter")
        if filt is not None and not isinstance(filt, str):
            _fail(
                where + ".filter",
                "validation",
                "invalid_field_type",
                "REQ-0287",
                {"expected": "str", "actual": _type_name(filt)},
            )
        if isinstance(filt, str):
            driver = e.spec.get("base")
            if driver is None and len(e.inputs) == 1:
                driver = next(iter(e.inputs))
            if is_self:
                donor_fields = set(e.donor_fields)
            else:
                donor_fields = set(e.inputs[ds].fields) | set(
                    d.get("derivations") or {}
                )
            driver_fields = (
                set(e.inputs[driver].fields) if driver in e.inputs else set()
            )

            def _ffail(ident, cond, ctx=None, _where=where):
                _fail(
                    _where + ".filter",
                    "validation",
                    "unknown_field",
                    cond,
                    {"name": ident, **(ctx or {})},
                )

            try:
                _, idents = _pred.parse(filt, where + ".filter")
            except YamaaError:
                idents = []
            for ident in idents:
                parts = ident.split(".")
                field = parts[-1]
                if (
                    is_self
                    and (len(parts) == 1 or (len(parts) == 2 and parts[0] == "SELF"))
                    or len(parts) == 2
                    and parts[0] == ds
                ):
                    if field not in donor_fields:
                        _ffail(ident, "REQ-0133")
                elif len(parts) == 2 and parts[0] == driver:
                    if field not in driver_fields:
                        _ffail(ident, "REQ-0133")
                else:
                    ctx = {}
                    if field in donor_fields:
                        ctx["suggestion"] = f"{ds}.{field}"
                    _ffail(ident, "REQ-0132", ctx)
        for f in ("value", "lower", "upper"):
            b = d.get("between") or {}
            if f in b and not isinstance(b[f], str):
                _fail(
                    where + ".between." + f,
                    "validation",
                    "invalid_field_type",
                    "REQ-0322",
                    {"expected": "variable", "actual": _type_name(b[f])},
                )
        if is_self:
            for i, term in enumerate(d.get("order_by") or []):
                v = term if isinstance(term, str) else term.get("variable")
                if not isinstance(v, str):
                    continue
                parts = v.split(".")
                ok = len(parts) == 1 or (len(parts) == 2 and parts[0] == "SELF")
                if not ok or parts[-1] not in e.donor_fields:
                    _fail(
                        f"{where}.order_by[{i}]",
                        "validation",
                        "unknown_field",
                        "REQ-0120",
                        {"name": v},
                    )
        _check_intermediate_derivations(e, d, where, ds)
        _check_intermediate_verification(e, d, where, ds)
    _check_intermediate_cycles(e)


def _check_intermediate_derivations(e, d, where, ds):
    """REQ-1185: derivations read the donor's stored fields (bare or
    dataset-qualified) plus earlier sibling derivations; windows allowed.
    REQ-1263: a non-window derivation may read another intermediate's
    column, matched from the donor record."""
    derivs = d.get("derivations")
    if derivs is None:
        return
    is_self = ds == "SELF"
    fields = set(e.donor_fields) if is_self else set(e.inputs[ds].fields)
    if not isinstance(derivs, dict):
        _fail(
            where + ".derivations",
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "map", "actual": _type_name(derivs)},
        )
    for name in derivs:
        if name in fields:
            _fail(
                f"{where}.derivations.{name}",
                "validation",
                "duplicate_derivation",
                "REQ-1185",
                {"name": name},
            )
    names = list(derivs)
    for idx, (name, deriv) in enumerate(derivs.items()):
        dpath = f"{where}.derivations.{name}"
        node = _norm(deriv, dpath)
        _check_tree(e, node, dpath, "row")
        bare, qual = _ident_refs(node)
        available = fields | set(names[:idx])
        for ref in sorted(bare):
            if ref not in available:
                _fail(dpath, "validation", "unknown_field", "REQ-1185", {"name": ref})
        window = _top_key(node) in _WINDOW_KINDS
        for ref in sorted(qual):
            head, _, field = ref.partition(".")
            if head == ds and field in available:
                continue
            other = e.lookups_decl.get(head)
            if other is None or other is d or window:
                _fail(dpath, "validation", "unknown_field", "REQ-1185", {"name": ref})
            _check_intermediate_read(e, other, field, ds, available, dpath)


def _check_intermediate_read(e, other, field, ds, available, dpath):
    """REQ-1263: the read intermediate reads an input dataset, carries the
    read column, and matches on names the donor record supplies."""
    if other["dataset"] == "SELF":
        _fail(
            dpath,
            "validation",
            "phase_boundary",
            "REQ-1263",
            {"intermediate": other["id"], "dataset": "SELF"},
        )
    table = e.inputs.get(other["dataset"])
    stored = set(table.fields) if table is not None else set()
    ofields = stored | set(other.get("derivations") or {})
    cols = other.get("columns")
    if field not in ofields or (cols is not None and field not in cols):
        _fail(
            dpath,
            "validation",
            "unknown_field",
            "REQ-1263",
            {"intermediate": other["id"], "column": field},
        )
    key = other.get("key")
    if key is None:
        vals = [k for k in e.keys if k in ofields]
    elif isinstance(key, dict):
        vals = list(key.values())
    else:
        vals = [key] if isinstance(key, str) else list(key)
    between = other.get("between") or {}
    if "value" in between:
        vals.append(between["value"])
    names = []
    for v in vals:
        if isinstance(v, str):
            names.append(v)
        elif isinstance(v, dict):
            b, q = _safe_idents(v)
            names.extend(b | q)
    for n in names:
        head, dot, fld = n.partition(".")
        ok = fld in available if dot else n in available
        if not ok or (dot and head != ds):
            _fail(
                dpath,
                "validation",
                "unknown_field",
                "REQ-1263",
                {"intermediate": other["id"], "name": n},
            )


def _check_intermediate_cycles(e):
    """REQ-1263: intermediates whose derivations read each other, directly
    or through a chain, fail as dependency_cycle."""
    reads = {}
    for d in e.spec.get("intermediates", []) or []:
        out = set()
        for deriv in (d.get("derivations") or {}).values():
            _, qual = _safe_idents(_safe_norm(deriv, ""))
            out |= {q.partition(".")[0] for q in qual} & set(e.lookups_decl)
        reads[d["id"]] = out
    state, stack = {}, []

    def visit(n):
        state[n] = "temp"
        stack.append(n)
        for r in sorted(reads.get(n, ())):
            if state.get(r) == "temp":
                cyc = stack[stack.index(r) :] + [r]
                _fail(
                    list(
                        dict.fromkeys(e.lookup_paths[c] + ".derivations" for c in cyc)
                    ),
                    "validation",
                    "dependency_cycle",
                    "REQ-1263",
                    {"cycle": cyc},
                )
            if state.get(r) is None:
                visit(r)
        stack.pop()
        state[n] = "perm"

    for n in reads:
        if state.get(n) is None:
            visit(n)


def intermediate_uniques(d, where):
    """REQ-1245: an intermediate's `unique` checks as (path, columns).
    `unique: [...]` is an unnamed check; `unique: {columns: [...]}` may
    name one with `id`."""
    out = []
    for j, v in enumerate(d.get("verifications") or ()):
        uniq = v["unique"]
        cols = uniq["columns"] if isinstance(uniq, dict) else uniq
        out.append((f"{where}.verifications[{j}].unique", cols))
    return out


def _check_intermediate_verification(e, d, where, ds):
    """REQ-1245: `verifications` lists `unique` checks over the donor
    records. A declared id is unique within the list (REQ-0398), and a
    correlated filter cannot combine with any check."""
    vers = d.get("verifications")
    if vers is None:
        return
    if not isinstance(vers, list):
        _fail(
            where + ".verifications",
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "list[intermediate_verification]", "actual": _type_name(vers)},
        )
    fields = set(e.donor_fields) if ds == "SELF" else set(e.inputs[ds].fields)
    derived = set(d.get("derivations") or {})
    ids = set()
    for j, v in enumerate(vers):
        vpath = f"{where}.verifications[{j}]"
        if not isinstance(v, dict) or len(v) != 1:
            _fail(
                vpath,
                "validation",
                "invalid_field_type",
                "REQ-0286",
                {"expected": "one verification keyword", "actual": _type_name(v)},
            )
        if "unique" not in v:
            _fail(
                vpath,
                "validation",
                "unknown_field",
                "REQ-0286",
                {"verification": next(iter(v))},
            )
        upath = vpath + ".unique"
        uniq = v["unique"]
        cols, cpath = uniq, upath
        if isinstance(uniq, dict):
            for f in uniq:
                if f not in ("columns", "id"):
                    _fail(
                        f"{upath}.{f}",
                        "validation",
                        "unknown_field",
                        "REQ-0285",
                        {"field": f},
                    )
            cols, cpath = uniq.get("columns"), upath + ".columns"
            if "id" in uniq:
                vid = uniq["id"]
                if not isinstance(vid, str) or not vid:
                    _fail(
                        upath + ".id",
                        "validation",
                        "invalid_field_type",
                        "REQ-0287",
                        {"expected": "verification_id", "actual": _type_name(vid)},
                    )
                if vid in ids:
                    _fail(
                        upath + ".id",
                        "validation",
                        "duplicate_identifier",
                        "REQ-0398",
                        {"identifier": vid, "intermediate": d.get("id")},
                    )
                ids.add(vid)
        if (
            not isinstance(cols, list)
            or not cols
            or any(not isinstance(c, str) for c in cols)
        ):
            _fail(
                cpath,
                "validation",
                "invalid_field_type",
                "REQ-0287",
                {"expected": "non-empty list[str]", "actual": _type_name(cols)},
            )
        for k, c in enumerate(cols):
            if c not in fields and c not in derived:
                _fail(
                    f"{cpath}[{k}]",
                    "validation",
                    "unknown_field",
                    "REQ-1245",
                    {"intermediate": d.get("id"), "name": c},
                )
    filt = d.get("filter")
    if vers and isinstance(filt, str):
        try:
            _, idents = _pred.parse(filt, where + ".filter")
        except YamaaError:
            idents = []
        if ds == "SELF":
            correlated = any("." in i and i.split(".")[0] != "SELF" for i in idents)
        else:
            correlated = any(i.split(".")[0] != ds for i in idents)
        if correlated:
            _fail(
                where + ".verifications[0].unique",
                "validation",
                "correlated_filter_with_unique_verification",
                "REQ-1245",
                {"intermediate": d.get("id")},
            )


def _check_rows(e):
    if (e.spec.get("rows") or []) and e.spec.get("filter") is not None:
        _fail(
            "filter",
            "validation",
            "prohibited_construct",
            "REQ-1042",
            {"conflicts_with": "rows"},
        )
    filt = e.spec.get("filter")
    if filt is not None and not isinstance(filt, str):
        _fail(
            "filter",
            "validation",
            "invalid_field_type",
            "REQ-1042",
            {"expected": "predicate", "actual": _type_name(filt)},
        )
    for t in e.spec.get("rows", []) or []:
        ds = t.get("dataset") or e._default_dataset()
        where = f"rows.{t['id']}"
        if ds not in e.inputs:
            _fail(
                where + ".dataset",
                "validation",
                "unknown_field",
                "REQ-0103",
                {"dataset": ds},
            )
        for g in t.get("group_by", []) or []:
            if not isinstance(g, str) or "." not in g:
                _fail(
                    where + ".group_by",
                    "validation",
                    "unknown_field",
                    "REQ-0066",
                    {"group_by": g},
                )


def _check_row_windows(e):
    """REQ-0326: a window used during row construction must not depend on"""
    for t in e.spec.get("rows", []) or []:
        derivs = t.get("derivations") or {}
        where = f"rows.{t['id']}.derivations"
        nodes = {}
        for dname, d in derivs.items():
            nodes[dname] = _norm(d, f"{where}.{dname}")
        wins = {
            n
            for n, nd in nodes.items()
            if isinstance(nd, dict) and len(nd) == 1 and next(iter(nd)) in _WINDOW_KINDS
        }
        if not wins:
            continue
        dep_cache = {}

        def closure(name, seen=None, _dep_cache=dep_cache, _nodes=nodes):
            if name in _dep_cache:
                return _dep_cache[name]
            seen = seen or set()
            if name in seen or name not in _nodes:
                return set()
            seen = seen | {name}
            out = set()
            for r in _unqualified_refs(_nodes[name]):
                out.add(r)
                out |= closure(r, seen)
            _dep_cache[name] = out
            return out

        for w in sorted(wins):
            bad = [
                r
                for r in sorted(_unqualified_refs(nodes[w]))
                if r in wins or closure(r) & wins
            ]
            if bad:
                _fail(
                    f"{where}.{w}.{next(iter(nodes[w]))}",
                    "validation",
                    "window_on_window_result",
                    "REQ-0326",
                    {"column": w, "depends_on": bad},
                )


def _check_derivations(e):
    for name in e.col_order:
        cs = e.colspecs[name]
        d = cs.get("derivation")
        if d is None:
            continue
        _check_tree(
            e,
            _norm(d, f"columns.{name}.derivation"),
            f"columns.{name}.derivation",
            "col",
        )
    for ti, t in enumerate(e.spec.get("rows", []) or []):
        grouped = bool(t.get("group_by"))
        first = ti == 0
        for dname, d in (t.get("derivations") or {}).items():
            node = _norm(d, f"rows.{t['id']}.derivations.{dname}")
            _, qual = _ident_refs(node)
            for q in sorted(qual):
                head = q.split(".")[0]
                if head == "SELF":
                    _fail(
                        f"rows.{t['id']}.derivations.{dname}",
                        "validation",
                        "prohibited_construct",
                        "REQ-0120",
                        {"identifier": q},
                    )
                # REQ-0120: the first template has no completed donor rows.
                other = e.lookups_decl.get(head)
                if first and other is not None and other.get("dataset") == "SELF":
                    _fail(
                        f"rows.{t['id']}.derivations.{dname}",
                        "validation",
                        "phase_boundary",
                        "REQ-0120",
                        {"intermediate": head, "row": t["id"]},
                    )
            _check_tree(
                e,
                node,
                f"rows.{t['id']}.derivations.{dname}",
                "row",
                row_grouped=grouped,
            )


def _check_tree(e, node, path, phase, row_grouped=False):
    if isinstance(node, str):
        node = {"source": node}
    if isinstance(node, dict) and "value" in node and set(node) <= _expr.HANDLED_FIELDS:
        if "unconvertible" in node and isinstance(node["unconvertible"], (dict, list)):
            _fail(
                path + ".unconvertible",
                "validation",
                "invalid_field_type",
                "REQ-0287",
                {"expected": "literal", "actual": _type_name(node["unconvertible"])},
            )
        _check_tree(e, node["value"], path + ".value", phase, row_grouped=row_grouped)
        return
    if not isinstance(node, dict) or len(node) != 1:
        _fail(
            path,
            "validation",
            "invalid_field_type",
            _expr.derivation_requirement(node),
            {"derivation": node},
        )
    key = next(iter(node))
    payload = node[key]
    sub = path + "." + key
    _check_expr(e, key, payload, sub, phase, row_grouped=row_grouped)
    if key == "value":
        _check_tree(e, payload, sub + ".value", phase)
    elif key == "case":
        _check_case(e, payload, sub, phase, row_grouped=row_grouped)
    elif key == "flag":
        _check_flag(e, payload, sub, phase, row_grouped=row_grouped)


def _check_flag(e, payload, path, phase, row_grouped=False):
    if isinstance(payload, str):
        cond, site = payload, path
    elif isinstance(payload, dict):
        cond = payload.get("condition")
        site = path + ".condition"
        if not isinstance(cond, str):
            _fail(
                site,
                "validation",
                "invalid_field_type",
                "REQ-1257",
                {"expected": "predicate", "actual": _type_name(cond)},
            )
        if "false_value" in payload and "missing" not in payload:
            _fail(
                path + ".missing",
                "validation",
                "missing_value_required",
                "REQ-1258",
                {},
            )
    else:
        _fail(
            path,
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "predicate", "actual": _type_name(payload)},
        )
    _pred.parse(cond, site)
    _check_predicate_types(e, cond, site)


def _var_type(e, var):
    """Declared type of a variable reference, or None if not statically known."""
    if "." in var:
        parts = var.split(".")
        if parts[0] in e.lookups_decl:
            return None
        table = e.inputs.get(parts[0])
        if table is None:
            return None
        return table.types.get(parts[-1])
    cs = e.colspecs.get(var)
    return cs.get("type") if cs else None


def _kind(t):
    if t in ("int", "float", "numeric"):
        return "num"
    return t


def _lit_type(v):
    if v is None:
        return None
    if isinstance(v, bool):
        return "bool"
    if isinstance(v, (int, float)):
        return "num"
    if isinstance(v, str):
        return "str"
    if isinstance(v, YDateTime):
        return "datetime"
    if isinstance(v, YDate):
        return "date"
    return None


def _check_input_type(e, var, expected, path, requirement):
    """Fail validation/incompatible_input_type when a variable's declared type
    is known and does not match the expected input type."""
    _check_input_types(e, var, [expected], path, requirement)


def _check_input_types(e, var, expected, path, requirement):
    """Plural form: the variable's declared type must match one of the
    expected input types (REQ-1107: to_date takes datetime or str)."""
    if not isinstance(var, str):
        return
    t = _var_type(e, var)
    if t is None or any(_kind(t) == _kind(x) for x in expected):
        return
    _fail(
        path,
        "validation",
        "incompatible_input_type",
        requirement,
        {"source": var, "expected": "|".join(expected), "actual": t},
    )


def _operand_static_type(e, operand):
    tag = operand[0]
    val = operand[1]
    if tag == "ident":
        return _var_type(e, val)
    return _lit_type(val)


def _check_predicate_types(e, text, path):
    """REQ-0190: operands of a comparison must have comparable static types."""
    try:
        ast, _ = _pred.parse(text, path)
    except YamaaError:
        return

    def check_pair(l, r):
        lt = _operand_static_type(e, l)
        rt = _operand_static_type(e, r)
        if lt is not None and rt is not None and _kind(lt) != _kind(rt):
            _fail(
                path,
                "validation",
                "incompatible_input_type",
                "REQ-0190",
                {"left_type": lt, "right_type": rt},
            )

    def walk(nd):
        if not isinstance(nd, tuple):
            return
        tag = nd[0]
        if tag == "cmp":
            check_pair(nd[2], nd[3])
        elif tag == "in":
            for item in nd[2]:
                check_pair(nd[1], item)
        elif tag == "between":
            check_pair(nd[1], nd[2])
            check_pair(nd[1], nd[3])
        elif tag == "like":
            lt = _operand_static_type(e, nd[1])
            if lt is not None and _kind(lt) != "str":
                _fail(
                    path,
                    "validation",
                    "incompatible_input_type",
                    "REQ-0190",
                    {"left_type": lt, "right_type": "str"},
                )
        else:
            for child in nd[1:]:
                walk(child)

    walk(ast)


def _check_case(e, payload, path, phase, row_grouped=False):
    if not isinstance(payload, list) or not payload:
        _fail(path, "validation", "invalid_field_type", "REQ-0339", {})
    seen_otherwise = False
    for i, item in enumerate(payload):
        ip = f"{path}[{i}]"
        if not isinstance(item, dict):
            _fail(ip, "validation", "invalid_field_type", "REQ-0339", {})
        if "otherwise" in item:
            if seen_otherwise or i != len(payload) - 1 or len(item) != 1:
                _fail(ip, "validation", "invalid_field_type", "REQ-0339", {})
            seen_otherwise = True
            _check_tree(
                e, item["otherwise"], ip + ".otherwise", phase, row_grouped=row_grouped
            )
        elif "when" in item and "then" in item:
            w = item["when"]
            if not isinstance(w, str):
                _fail(
                    ip + ".when",
                    "validation",
                    "invalid_field_type",
                    "REQ-0287",
                    {"expected": "str", "actual": _type_name(w)},
                )
            _pred.parse(w, ip + ".when")
            _check_predicate_types(e, w, ip + ".when")
            _check_tree(e, item["then"], ip + ".then", phase, row_grouped=row_grouped)
        else:
            _fail(ip, "validation", "invalid_field_type", "REQ-0339", {})


def _check_expr(e, key, payload, path, phase, row_grouped=False):
    if key in _VARIABLE_FIELDS:
        for f in _VARIABLE_FIELDS[key]:
            if (
                isinstance(payload, dict)
                and f in payload
                and not isinstance(payload[f], str)
            ):
                v = payload[f]
                _fail(
                    f"{path}.{f}",
                    "validation",
                    "invalid_field_type",
                    "REQ-0287",
                    {
                        "expected": "variable",
                        "actual": "mapping" if isinstance(v, dict) else _type_name(v),
                    },
                )
    if key == "source" and isinstance(payload, dict):
        ob, keep = payload.get("order_by"), payload.get("keep")
        if bool(ob) != bool(keep):
            _fail(
                path,
                "validation",
                "unpaired_fields",
                "REQ-0119",
                {
                    "declared": ["order_by"] if ob else ["keep"],
                    "missing": ["keep"] if ob else ["order_by"],
                },
            )
    if key == "source" and isinstance(payload, dict) and "filter" in payload:
        f = payload["filter"]
        if not isinstance(f, str):
            _fail(
                f"{path}.filter",
                "validation",
                "invalid_field_type",
                "REQ-0287",
                {"expected": "str", "actual": _type_name(f)},
            )
        else:
            var = payload.get("variable")
            if isinstance(var, str) and "." not in var:
                _fail(
                    f"{path}.filter",
                    "validation",
                    "prohibited_construct",
                    "REQ-0148",
                    {"identifier": var},
                )
            ds = var.split(".")[0] if isinstance(var, str) else None
            fields = None
            if ds is not None and ds in e.inputs:
                fields = set(e.inputs[ds].fields)
            try:
                _fast, names = _pred.parse(f, f"{path}.filter")
            except YamaaError:
                names = []
            for nm in names:
                if "." in nm:
                    qds, fld = nm.split(".", 1)
                    ok = qds == ds and fields is not None and fld in fields
                else:
                    ok = fields is not None and nm in fields
                    qds = ds
                if not ok:
                    _fail(
                        f"{path}.filter",
                        "validation",
                        "unknown_field",
                        "REQ-0132",
                        {"identifier": nm, "dataset": ds},
                    )
    if key in _WINDOW_KINDS and isinstance(payload, dict):
        _check_window(e, key, payload, path)
    if key == "aggregate" and isinstance(payload, dict):
        if phase == "row" and row_grouped and payload.get("key") is not None:
            _fail(
                f"{path}.key",
                "validation",
                "invalid_aggregate_context",
                "REQ-0142",
                {"expr": payload.get("expr")},
            )
        if phase == "col":
            _check_aggregate_key(e, payload, path)
        b = payload.get("between") or {}
        for f in ("value", "lower", "upper"):
            if f in b and not isinstance(b[f], str):
                _fail(
                    f"{path}.between.{f}",
                    "validation",
                    "invalid_field_type",
                    "REQ-0322",
                    {"expected": "variable", "actual": _type_name(b[f])},
                )
        ex = payload.get("expr")
        if isinstance(ex, str):
            _agg.parse(ex, path + ".expr")
    if key == "compute" and isinstance(payload, dict):
        ex = payload.get("expr")
        if isinstance(ex, str):
            _node, names = _numeric.parse(ex, path + ".expr")
            if phase == "col":
                lookups = {d.get("id") for d in e.spec.get("intermediates", []) or []}
                for n in names:
                    if "." in n and n.split(".")[0] not in lookups:
                        _fail(
                            path + ".expr",
                            "validation",
                            "qualified_identifier",
                            "REQ-0442",
                            {"expr": ex, "identifier": n},
                        )
    if isinstance(payload, dict):
        if key == "cut":
            _check_input_type(
                e, payload.get("source"), "numeric", path + ".source", "REQ-0306"
            )
        elif key == "str_case":
            _check_input_type(
                e, payload.get("source"), "str", path + ".source", "REQ-0308"
            )
            cases = ["upper", "lower", "sentence", "title"]
            if payload.get("to") not in cases:
                _fail(
                    path + ".to",
                    "validation",
                    "value_not_permitted",
                    "REQ-0287",
                    {"value": payload.get("to"), "permitted": cases},
                )
        elif key == "round_half_away_from_zero":
            _check_input_type(
                e, payload.get("source"), "numeric", path + ".source", "REQ-0418"
            )
            if not isinstance(payload.get("digits"), int) or isinstance(
                payload.get("digits"), bool
            ):
                _fail(
                    path + ".digits",
                    "validation",
                    "invalid_field_type",
                    "REQ-1172",
                    {"expected": "int", "actual": _type_name(payload.get("digits"))},
                )
        elif key in ("str_extract", "str_contains"):
            _check_input_type(
                e, payload.get("source"), "str", path + ".source", "REQ-0308"
            )
            if key == "str_contains" and payload.get("pattern") is not None:
                _check_portable_pattern(payload["pattern"], f"{path}.pattern")
        elif key == "str_pad":
            src = payload.get("source")
            if not isinstance(src, str):
                _fail(
                    path + ".source",
                    "validation",
                    "invalid_field_type",
                    "REQ-0322",
                    {"expected": "variable", "actual": _type_name(src)},
                )
            w = payload.get("width")
            if isinstance(w, bool) or not isinstance(w, int) or w < 1:
                _fail(
                    path + ".width",
                    "validation",
                    "invalid_field_type",
                    "REQ-1261",
                    {"width": w},
                )
        elif key == "to_date":
            _check_input_types(
                e,
                payload.get("source"),
                ["datetime", "str"],
                path + ".source",
                "REQ-0607",
            )
        elif key == "date_diff":
            _check_input_type(
                e, payload.get("start"), "date", path + ".start", "REQ-0606"
            )
            _check_input_type(e, payload.get("end"), "date", path + ".end", "REQ-0606")
        elif key == "study_day":
            _check_input_type(
                e, payload.get("date"), "date", path + ".date", "REQ-0606"
            )
            _check_input_type(
                e, payload.get("reference"), "date", path + ".reference", "REQ-0606"
            )
        elif key == "aggregate":
            ex = payload.get("expr")
            if isinstance(ex, str):
                try:
                    anode, _ = _agg.parse(ex, path + ".expr")
                except YamaaError:
                    anode = None

                def _num_idents(nd, out):
                    if isinstance(nd, tuple):
                        if nd[0] == "ident":
                            out.append(nd[1])
                        else:
                            for c in nd[1:]:
                                _num_idents(c, out)
                    elif isinstance(nd, list):
                        for c in nd:
                            _num_idents(c, out)

                def _walk_red(nd):
                    if not isinstance(nd, tuple):
                        return
                    if nd[0] == "reduce" and nd[1] in (
                        "SUM",
                        "MEAN",
                        "STDEV",
                        "STDEVP",
                        "VAR",
                        "VARP",
                    ):
                        names = []
                        _num_idents(nd[2], names)
                        for nm in names:
                            _check_input_type(
                                e, nm, "numeric", path + ".expr", "REQ-0510"
                            )
                    else:
                        for c in nd[1:]:
                            _walk_red(c)

                if anode is not None:
                    _walk_red(anode)
        elif key in ("greatest", "least", "first_available"):
            srcs = payload.get("sources") if isinstance(payload, dict) else None
            if not isinstance(srcs, list):
                srcs = [payload] if isinstance(payload, str) else []
            kinds = {}
            for s in srcs:
                if isinstance(s, dict):
                    s = s.get("value", s.get("source"))
                if isinstance(s, str):
                    t = _var_type(e, s)
                    if t is not None:
                        kinds[s] = _kind(t)
            if len(set(kinds.values())) > 1:
                _fail(
                    path,
                    "validation",
                    "incomparable_sources",
                    "REQ-0324",
                    {"sources": list(kinds), "types": list(kinds.values())},
                )
    if key == "literal" and isinstance(payload, (dict, list)):
        _fail(
            path,
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "literal_value", "actual": _type_name(payload)},
        )
    if key == "str_extract" and isinstance(payload, dict):
        pat = payload.get("pattern")
        if pat is not None:
            _check_portable_pattern(pat, f"{path}.pattern")
            rx = _normalize_pattern(pat)
            g = payload.get("group", 0)
            if not isinstance(g, int) or isinstance(g, bool) or g < 0 or g > rx.groups:
                _fail(
                    f"{path}.group",
                    "validation",
                    "regex_group_out_of_range",
                    "REQ-0828",
                    {"group": g, "groups": rx.groups},
                )
    if key == "str_template":
        t = payload if isinstance(payload, str) else payload.get("template")
        if not isinstance(t, str):
            _fail(
                path,
                "validation",
                "invalid_field_type",
                "REQ-0287",
                {"expected": "str", "actual": _type_name(t)},
            )
    if key == "date_impute" and isinstance(payload, dict):
        prec = payload.get("minimum_source_precision", "year")
        m = payload.get("month")
        if prec == "month" and m is not None:
            _fail(
                f"{path}.month",
                "validation",
                "month_not_permitted",
                "REQ-0592",
                {"month": m},
            )
        if prec != "month" and m is None:
            _fail(
                f"{path}.month",
                "validation",
                "month_required",
                "REQ-0592",
                {"minimum_source_precision": prec},
            )
        if m is not None and (
            not isinstance(m, int) or isinstance(m, bool) or not 1 <= m <= 12
        ):
            _fail(
                f"{path}.month",
                "validation",
                "month_out_of_range",
                "REQ-0608",
                {"month": m},
            )
        day = payload.get("day")
        if isinstance(day, str):
            if day not in ("first", "last"):
                _fail(
                    f"{path}.day",
                    "validation",
                    "value_not_permitted",
                    "REQ-0609",
                    {"value": day, "permitted": ["first", "last"]},
                )
        elif isinstance(day, bool) or not isinstance(day, int) or not 1 <= day <= 31:
            _fail(
                f"{path}.day",
                "validation",
                "value_not_permitted",
                "REQ-0609",
                {"value": day, "permitted": ["first", "last", "1-31"]},
            )
    if key == "row_value" and isinstance(payload, dict):
        off = payload.get("offset")
        if not isinstance(off, int) or isinstance(off, bool) or off == 0:
            _fail(
                f"{path}.offset",
                "validation",
                "zero_offset",
                "REQ-0328",
                {"offset": off},
            )
    if key == "rank" and isinstance(payload, dict):
        m = payload.get("method", "competition")
        if not isinstance(m, str):
            _fail(
                f"{path}.method",
                "validation",
                "invalid_field_type",
                "REQ-0287",
                {"expected": "str", "actual": _type_name(m)},
            )
        if m not in ("competition", "dense"):
            _fail(
                f"{path}.method",
                "validation",
                "value_not_permitted",
                "REQ-0287",
                {"value": m, "permitted": ["competition", "dense"]},
            )
    if key == "mapping" and isinstance(payload, dict):
        # REQ-1110: dict is inline or a YAML path; dict_yaml is dropped.
        if "dict_yaml" in payload:
            _fail(
                path,
                "validation",
                "unknown_field",
                "REQ-1110",
                {"field": "dict_yaml"},
            )
        if "dict" not in payload:
            _fail(
                path,
                "validation",
                "invalid_field_type",
                "REQ-1110",
                {"expected": "dict", "actual": "absent"},
            )
        d = payload["dict"]
        if not isinstance(d, (dict, str)):
            _fail(
                f"{path}.dict",
                "validation",
                "invalid_field_type",
                "REQ-1110",
                {"expected": "dict or path", "actual": _type_name(d)},
            )


def _check_aggregate_key(e, payload, path):
    """REQ-0115/REQ-0141: every `key` column is a column of the qualified
    relation, written bare, and every plain match value names a known
    current-row value."""
    key = payload.get("key")
    if key is None:
        return
    heads = set()
    ex = payload.get("expr")
    if isinstance(ex, str):
        try:
            _, names = _agg.parse(ex, path + ".expr")
        except YamaaError:
            names = []
        heads |= {n.split(".")[0] for n in names if "." in n}
    for b in payload.get("derive") or []:
        if isinstance(b, dict):
            _, qual = _safe_idents(_safe_norm(b.get("derivation"), path))
            heads |= {q.split(".")[0] for q in qual}
    rel = sorted(h for h in heads if h in e.inputs)
    if len(rel) != 1:
        return
    fields = set(e.inputs[rel[0]].fields)
    if isinstance(key, str):
        pairs = [(key, key)]
    elif isinstance(key, dict):
        pairs = list(key.items())
    elif isinstance(key, list):
        pairs = [(k, k) for k in key]
    else:
        _fail(
            path + ".key",
            "validation",
            "invalid_field_type",
            "REQ-0115",
            {"expected": "list or mapping", "actual": _type_name(key)},
        )
    if not pairs:
        _fail(
            path + ".key",
            "validation",
            "missing_aggregate_keys",
            "REQ-0140",
            {"dataset": rel[0]},
        )
    for col, mv in pairs:
        if not isinstance(col, str) or col not in fields:
            _fail(
                path + ".key",
                "validation",
                "unknown_field",
                "REQ-0141",
                {"key": col, "dataset": rel[0]},
            )
        if isinstance(mv, str) and "." not in mv and mv not in e.colspecs:
            _fail(
                path + ".key",
                "validation",
                "unknown_field",
                "REQ-0141",
                {"identifier": mv},
            )


def check_window_shape(w, where):
    """REQ-1251: validate a window_spec mapping's shape. Used for inline"""
    if not isinstance(w, dict):
        _fail(
            where,
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "object", "actual": _type_name(w)},
        )
    gb = w.get("group_by", [])
    if not isinstance(gb, list):
        _fail(
            where + ".group_by",
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "list[variable]", "actual": _type_name(gb)},
        )
    for g in gb:
        if not isinstance(g, str):
            _fail(
                where + ".group_by",
                "validation",
                "invalid_field_type",
                "REQ-0322",
                {"expected": "variable", "actual": _type_name(g)},
            )
    ob = w.get("order_by", [])
    if not isinstance(ob, list):
        _fail(
            where + ".order_by",
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "list[order_by_term]", "actual": _type_name(ob)},
        )
    for i, t in enumerate(ob):
        v = (
            t
            if isinstance(t, str)
            else (t.get("variable") if isinstance(t, dict) else None)
        )
        if not isinstance(v, str):
            _fail(
                f"{where}.order_by[{i}]",
                "validation",
                "invalid_field_type",
                "REQ-0322",
                {"expected": "variable", "actual": _type_name(t)},
            )
    filt = w.get("filter")
    if filt is not None and not isinstance(filt, str):
        _fail(
            where + ".filter",
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "str", "actual": _type_name(filt)},
        )


def check_named_window_definition(name, w):
    """REQ-1251/1253: a named window definition is an inline mapping, never"""
    where = f"windows.{name}"
    if not isinstance(w, dict):
        _fail(
            where,
            "validation",
            "invalid_field_type",
            "REQ-1251",
            {"expected": "window_spec", "actual": _type_name(w), "name": name},
        )
    for k in w:
        if k not in ("group_by", "order_by", "filter"):
            _fail(
                where,
                "validation",
                "invalid_field_type",
                "REQ-1253",
                {"field": f"windows.{name}.{k}", "expected": "window_spec field"},
            )
    check_window_shape(w, where)


def _check_window(e, key, payload, path):
    w = payload.get("window", {}) or {}
    check_window_shape(w, path + ".window")
    if key in (
        "row_number",
        "rank",
        "row_value",
        "previous_non_missing",
        "locf",
    ) and not w.get("order_by"):
        _fail(
            path + ".window",
            "validation",
            "window_order_by_required",
            "REQ-0340",
            {"operation": key},
        )
    if key == "baseline_flag" and w.get("order_by"):
        _fail(
            path,
            "validation",
            "prohibited_construct",
            "REQ-0341",
            {"field": "window.order_by"},
        )


def _ident_refs(node):
    """(bare, qualified) variable references in an expression tree. Unlike"""
    bare, qual = set(), set()

    def add(v):
        if isinstance(v, str):
            (qual if "." in v else bare).add(v)

    def walk(n):
        if isinstance(n, str):
            return
        if not isinstance(n, dict) or len(n) != 1:
            return
        key = next(iter(n))
        payload = n[key]
        if key == "value":
            walk(payload)
        elif key == "source":
            add(payload["variable"] if isinstance(payload, dict) else payload)
        elif key in ("greatest", "least", "first_available"):
            for s in payload["sources"]:
                add(s if isinstance(s, str) else s.get("variable"))
        elif key == "mapping":
            src = payload["source"]
            add(src if isinstance(src, str) else src.get("variable"))
        elif key in (
            "cut",
            "str_extract",
            "str_case",
            "str_contains",
            "str_pad",
            "to_date",
            "date_precision",
        ):
            add(payload["source"] if isinstance(payload, dict) else None)
        elif key == "flag":
            cond = payload if isinstance(payload, str) else payload.get("condition")
            if isinstance(cond, str):
                try:
                    _, names = _pred.parse(cond, "<idents>")
                except YamaaError:
                    names = []
                for nm in names:
                    add(nm)
        elif key == "date_impute":
            for f in ("source", "not_before"):
                add(payload.get(f))
        elif key == "case":
            for item in payload:
                if "when" in item:
                    _, names = _pred.parse(item["when"], "<idents>")
                    for nm in names:
                        add(nm)
                walk(item.get("then") or item.get("otherwise"))
        elif key == "compute":
            _, names = _numeric.parse(payload["expr"], "<idents>")
            for nm in names:
                add(nm)
        elif key == "str_template":
            t = payload if isinstance(payload, str) else payload["template"]
            for m in _expr._TEMPLATE_RE.finditer(t):
                tok = m.group()
                if tok not in ("{{", "}}"):
                    add(tok[1:-1])
        elif key == "str_concat":
            for s in payload["sources"]:
                walk(s)
        elif key == "aggregate":
            p = payload
            ex = p if isinstance(p, str) else p["expr"]
            _, names = _agg.parse(ex, "<idents>")
            for nm in names:
                add(nm)
            b = (p.get("between") or {}) if isinstance(p, dict) else {}
            add(b.get("value"))
        elif key == "date_diff":
            for f in ("start", "end"):
                add(payload[f])
        elif key == "study_day":
            for f in ("date", "reference"):
                add(payload[f])
        elif key in _WINDOW_KINDS:
            w = payload.get("window", {})
            for v in w.get("group_by", []) or []:
                add(v)
            for t in w.get("order_by", []) or []:
                add(t if isinstance(t, str) else t.get("variable"))
            if w.get("filter"):
                _, names = _pred.parse(w["filter"], "<idents>")
                for nm in names:
                    add(nm)
            for f in ("source", "date", "reference_date", "value", "flag"):
                if f in payload:
                    add(payload[f])
            b = payload.get("between") or {}
            add(b.get("value"))

    walk(node)
    return bare, qual


def _unqualified_refs(node, phase="col"):
    """Unqualified output-column references for dependency ordering."""
    refs = set()
    if isinstance(node, str):
        if "." not in node:
            refs.add(node)
        return refs
    if not isinstance(node, dict) or len(node) != 1:
        return refs
    key = next(iter(node))
    payload = node[key]
    if key == "value":
        return _unqualified_refs(payload, phase)
    if key == "source" and isinstance(payload, str) and "." not in payload:
        refs.add(payload)
    elif key in ("greatest", "least", "first_available"):
        for s in payload["sources"]:
            v = s if isinstance(s, str) else s.get("variable")
            if isinstance(v, str) and "." not in v:
                refs.add(v)
    elif key == "mapping":
        src = payload["source"]
        v = src if isinstance(src, str) else src.get("variable")
        if isinstance(v, str) and "." not in v:
            refs.add(v)
    elif key in (
        "cut",
        "str_extract",
        "str_case",
        "str_pad",
        "to_date",
        "date_precision",
    ):
        v = payload["source"] if isinstance(payload, dict) else None
        if isinstance(v, str) and "." not in v:
            refs.add(v)
    elif key == "flag":
        cond = payload if isinstance(payload, str) else payload.get("condition")
        if isinstance(cond, str):
            try:
                _, names = _pred.parse(cond, "<deps>")
            except YamaaError:
                names = []
            refs.update(n for n in names if "." not in n)
    elif key == "date_impute":
        for f in ("source", "not_before"):
            v = payload.get(f)
            if isinstance(v, str) and "." not in v:
                refs.add(v)
    elif key == "case":
        for item in payload:
            if "when" in item:
                _, names = _pred.parse(item["when"], "<deps>")
                refs.update(n for n in names if "." not in n)
            tgt = item.get("then") or item.get("otherwise")
            refs.update(_unqualified_refs(tgt, phase))
    elif key == "compute":
        _, names = _numeric.parse(payload["expr"], "<deps>")
        refs.update(n for n in names if "." not in n)
    elif key == "str_template":
        t = payload if isinstance(payload, str) else payload["template"]
        for m in _expr._TEMPLATE_RE.finditer(t):
            tok = m.group()
            if tok not in ("{{", "}}"):
                n = tok[1:-1]
                if "." not in n:
                    refs.add(n)
    elif key == "str_concat":
        for s in payload["sources"]:
            refs.update(_unqualified_refs(s, phase))
    elif key == "aggregate":
        p = payload
        ex = p if isinstance(p, str) else p["expr"]
        _, names = _agg.parse(ex, "<deps>")
        refs.update(n for n in names if "." not in n)
        b = (p.get("between") or {}) if isinstance(p, dict) else {}
        if b.get("value") and "." not in b["value"]:
            refs.add(b["value"])
    elif key == "date_diff":
        for f in ("start", "end"):
            v = payload[f]
            if isinstance(v, str) and "." not in v:
                refs.add(v)
    elif key == "study_day":
        for f in ("date", "reference"):
            v = payload[f]
            if isinstance(v, str) and "." not in v:
                refs.add(v)
    elif key in _WINDOW_KINDS:
        w = payload.get("window", {})
        for v in w.get("group_by", []) or []:
            if isinstance(v, str) and "." not in v:
                refs.add(v)
        for t in w.get("order_by", []) or []:
            v = t if isinstance(t, str) else t.get("variable")
            if isinstance(v, str) and "." not in v:
                refs.add(v)
        if w.get("filter"):
            _, names = _pred.parse(w["filter"], "<deps>")
            refs.update(n for n in names if "." not in n)
        for f in ("source", "date", "reference_date", "value", "flag"):
            if f in payload and isinstance(payload[f], str) and "." not in payload[f]:
                refs.add(payload[f])
    return refs


def _topo_order(derivs, ref_fn, where):
    order = []
    state = {}

    def visit(n):
        s = state.get(n)
        if s == "perm":
            return
        if s == "temp":
            _fail(where, "validation", "dependency_cycle", "REQ-0072", {"column": n})
        state[n] = "temp"
        for r in ref_fn(derivs[n]):
            if r in derivs:
                visit(r)
        state[n] = "perm"
        order.append(n)

    for n in derivs:
        visit(n)
    return order


def _top_key(node):
    if isinstance(node, dict) and len(node) == 1:
        return next(iter(node))
    return "source"


def _check_functions(e):
    """Stage 2: validate `function:` calls against the project environment."""
    if e.functions is None:
        return
    from . import functions as _functions

    def walk_derivs(derivs, where):
        for dname, d in (derivs or {}).items():
            node = _norm(d, f"{where}.{dname}")
            if isinstance(node, dict) and len(node) == 1 and "function" in node:
                _functions.check_call(
                    e.functions, node["function"], f"{where}.{dname}.function"
                )

    for name in e.col_order:
        d = e.colspecs[name].get("derivation")
        if d is None:
            continue
        node = _norm(d, f"columns.{name}.derivation")
        if isinstance(node, dict) and len(node) == 1 and "function" in node:
            _functions.check_call(
                e.functions, node["function"], f"columns.{name}.derivation.function"
            )
    for t in e.spec.get("rows", []) or []:
        walk_derivs(t.get("derivations"), f"rows.{t['id']}.derivations")


def _column_deps(e, safe=False):
    """Each column's top expression key and the columns it reads. With
    `safe`, a malformed derivation reads nothing; its own check reports it."""
    keys, deps = {}, {}
    for n in e.col_order:
        d = e.colspecs[n].get("derivation")
        node = None
        if d is not None:
            node = _safe_norm(d, f"columns.{n}.derivation") if safe else _norm(d, "")
        keys[n] = _top_key(node) if node else "source"
        try:
            refs = _unqualified_refs(node) if node else set()
        except Exception:
            if not safe:
                raise
            refs = set()
        deps[n] = {r for r in refs if r in e.colspecs}
    return keys, deps


def _check_cycles(e):
    """REQ-0072: a dependency cycle rejects the specification before any one
    derivation's operands are judged."""
    keys, deps = _column_deps(e, safe=True)
    state = {}
    stack = []

    def visit(n):
        state[n] = "temp"
        stack.append(n)
        for r in sorted(deps[n]):
            if state.get(r) == "temp":
                cyc = stack[stack.index(r) :] + [r]
                paths = [f"columns.{c}.derivation.{keys[c]}" for c in cyc]
                _fail(
                    list(dict.fromkeys(paths)),
                    "validation",
                    "dependency_cycle",
                    "REQ-0072",
                    {"cycle": cyc},
                )
            if state.get(r) is None:
                visit(r)
        stack.pop()
        state[n] = "perm"

    for n in e.col_order:
        if state.get(n) is None:
            visit(n)


def _check_dependencies(e):
    decl = e.col_order
    idx = {n: i for i, n in enumerate(decl)}
    keys, deps = _column_deps(e)
    for n in decl:
        for r in sorted(deps[n]):
            if idx[r] > idx[n]:
                _fail(
                    f"columns.{n}.derivation.{keys[n]}",
                    "validation",
                    "forward_reference",
                    "REQ-0071",
                    {"column": n, "dependency": r},
                )
    if not (e.spec.get("rows") or []):
        keyset = set(e.spec.get("keys") or [])
        for n in decl:
            if n in keyset:
                for r in sorted(deps[n]):
                    if r not in keyset:
                        _fail(
                            f"columns.{n}.derivation",
                            "validation",
                            "key_dependency",
                            "REQ-0074",
                            {"column": n, "dependency": r},
                        )


def _vlist(v):
    if v is None:
        return []
    if isinstance(v, dict):
        return [v]
    return v


def _check_verification_ids(entries, where):
    """REQ-0374/REQ-0398: an optional id is nonempty text and unique within
    its own verification list; another list may reuse it."""
    seen = set()
    for i, v in enumerate(entries):
        if not isinstance(v, dict) or len(v) != 1:
            continue
        kind, payload = next(iter(v.items()))
        if not isinstance(payload, dict) or "id" not in payload:
            continue
        vid = payload["id"]
        path = f"{where}[{i}].{kind}.id"
        if not isinstance(vid, str) or not vid:
            _fail(
                path,
                "validation",
                "invalid_field_type",
                "REQ-0287",
                {"expected": "verification_id", "actual": _type_name(vid)},
            )
        if vid in seen:
            _fail(path, "validation", "duplicate_identifier", "REQ-0398", {"id": vid})
        seen.add(vid)


def _check_verifications(e):
    for name in e.col_order:
        entries = _vlist(e.colspecs[name].get("verifications"))
        _check_verification_ids(entries, f"columns.{name}.verifications")
        for i, v in enumerate(entries):
            if not isinstance(v, dict) or len(v) != 1:
                continue
            kind, payload = next(iter(v.items()))
            payload = payload or {}
            where = f"columns.{name}.verifications[{i}].{kind}"
            if kind == "matches":
                pat = payload.get("pattern")
                _check_portable_pattern(pat, where + ".pattern")
            for f in ("expr", "when", "then", "filter"):
                if f in payload and not isinstance(payload[f], str):
                    _fail(
                        where + "." + f,
                        "validation",
                        "invalid_field_type",
                        "REQ-0287",
                        {"expected": "str", "actual": _type_name(payload[f])},
                    )
    entries = e.spec.get("verifications") or []
    _check_verification_ids(entries, "verifications")
    for i, v in enumerate(entries):
        if not isinstance(v, dict) or len(v) != 1:
            continue
        kind, payload = next(iter(v.items()))
        payload = payload or {}
        where = f"verifications[{i}].{kind}"
        if kind == "unique" and isinstance(payload, list):
            # REQ-0381: `unique: [...]` is the unnamed form of the check.
            payload = {"columns": payload}
        if kind == "unique":
            cols = payload.get("columns") if isinstance(payload, dict) else None
            if (
                not isinstance(cols, list)
                or not cols
                or any(not isinstance(c, str) for c in cols)
            ):
                _fail(
                    where,
                    "validation",
                    "invalid_field_type",
                    "REQ-0287",
                    {
                        "expected": "non-empty list[variable]",
                        "actual": _type_name(cols),
                    },
                )
        if not isinstance(payload, dict):
            continue
        for f in ("expr", "when", "then", "filter"):
            if f in payload and not isinstance(payload[f], str):
                _fail(
                    where + "." + f,
                    "validation",
                    "invalid_field_type",
                    "REQ-0287",
                    {"expected": "str", "actual": _type_name(payload[f])},
                )
