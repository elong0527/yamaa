"""Execute a specification end to end and compare against its expected CSV."""
from __future__ import annotations

import csv
from pathlib import Path

import yaml

import engine
from engine import (
    AGGREGATE_OPS,
    ODM_CONTEXT,
    SCALAR_OPS,
    WINDOW_OPS,
    DerivationError,
    Unmapped,
    convert,
    evaluate_predicate,
    predicate_vars,
    serialize,
)
from validate import EXC, OPS


def _as_list(v):
    return [] if v is None else (v if isinstance(v, list) else [v])


def _exception_map(deriv):
    """Exception name by R008 stage for one derivation."""
    out = {}
    for ex in _as_list(deriv.get("exception")):
        name, args = next(iter(ex.items()))
        out[EXC[name]["stage"]] = (name, args or {})
    return out


class Row:
    __slots__ = ("src", "driver", "template", "out")

    def __init__(self, src, driver, template):
        self.src = src
        self.driver = driver
        self.template = template
        self.out = {}


class Spec:
    def __init__(self, path):
        self.path = Path(path)
        self.spec = yaml.safe_load(self.path.read_text())
        self.base = self.spec.get("base")
        self.keys = self.spec.get("keys", [])
        self.columns = self.spec["columns"]
        self.coltype = {c["name"]: c["type"] for c in self.columns}
        self.order = [c["name"] for c in self.columns]
        self.data = {
            name: list(csv.DictReader(open(self.path.parent / rel)))
            for name, rel in self.spec["datasets"].items()
        }
        self.rows = []
        self.row_lookup = lambda name: None

    # -- reference resolution ------------------------------------------

    def _odm_item(self, ds, item, src):
        """R002 contextual reference: an ItemOID resolved within the current
        study, subject, event, item-group and repeat-key context."""
        ctx = {k: src.get(k) for k in ODM_CONTEXT if k in src}
        hits = [
            r
            for r in self.data[ds]
            if r.get("ItemOID") == item and all(r.get(k) == v for k, v in ctx.items())
        ]
        if len(hits) > 1:
            raise DerivationError(f"{ds}.{item}: {len(hits)} records match the context")
        return hits[0].get("Value") if hits else None

    def _joined(self, ds, col, row, deriv):
        """R003 left join on applicable keys, with optional right-side reduction."""
        right = self.data[ds]
        if deriv.get("filter"):
            right = [
                r for r in right if evaluate_predicate(deriv["filter"], _rec_lookup(r, ds))
            ]
        applicable = [k for k in self.keys if right and k in right[0]]
        if not applicable:
            raise DerivationError(f"{ds}.{col}: no applicable keys (R003)")

        ops = _as_list(deriv.get("operations"))
        reducing = ops and OPS[next(iter(ops[0]))]["kind"] == "aggregate"
        want = tuple(row.out.get(k) for k in applicable)
        hits = [r for r in right if tuple(r.get(k) for k in applicable) == want]

        if reducing:
            fn = AGGREGATE_OPS[next(iter(ops[0]))]
            return fn([h.get(col) for h in hits])
        if len(hits) > 1:
            stage = _exception_map(deriv).get("join")
            if not stage or stage[0] != "multiple_matches":
                raise DerivationError(f"{ds}.{col}: {len(hits)} right-side matches (R003)")
            _, args = stage
            ordered = sorted(hits, key=lambda h: str(h.get(_var(args["order_by"]).split(".")[-1])))
            hits = [ordered[-1] if args["keep"] == "last" else ordered[0]]
        return hits[0].get(col) if hits else None

    def resolve(self, ref, row, deriv):
        if "." not in ref:
            if ref not in row.out:
                raise DerivationError(f"{ref} is not available yet (R001)")
            return row.out[ref]
        ds, rest = ref.split(".", 1)
        if ds == row.driver:
            if rest in row.src:
                return row.src[rest]
            return self._odm_item(ds, rest, row.src)
        if rest in (self.data[ds][0] if self.data[ds] else {}):
            return self._joined(ds, rest, row, deriv)
        return self._odm_item(ds, rest, row.src)

    # -- dependencies ---------------------------------------------------

    def deps(self, deriv, driver):
        out = set()
        src = deriv.get("source")
        if src and "." not in str(src):
            out.add(src)
        for k in deriv.get("group_by") or []:
            out.add(k)
        if src and "." in str(src) and str(src).split(".")[0] != driver:
            # an R003 join needs its applicable keys first
            out.update(self.keys)
        for op in _as_list(deriv.get("operations")):
            out |= _arg_deps(next(iter(op.values())) or {})
        for ex in _as_list(deriv.get("exception")):
            out |= _arg_deps(next(iter(ex.values())) or {})
        return out

    # -- evaluation -----------------------------------------------------

    def _args(self, raw, row, deriv):
        if isinstance(raw, dict) and set(raw) == {"source"}:
            return self.resolve(raw["source"], row, deriv)
        if isinstance(raw, dict):
            return {k: self._args(v, row, deriv) for k, v in raw.items()}
        if isinstance(raw, list):
            return [self._args(v, row, deriv) for v in raw]
        return raw

    def evaluate(self, name, deriv, rows):
        exc = _exception_map(deriv)
        vals = []
        for row in rows:
            try:
                if "literal" in deriv:
                    v = deriv["literal"]
                elif "source" in deriv:
                    v = self.resolve(deriv["source"], row, deriv)
                    if v in (None, "") and "bind" in exc:
                        v = exc["bind"][1]["default"]
                else:
                    v = None
            except DerivationError:
                if "bind" not in exc:
                    raise
                v = exc["bind"][1]["default"]
            vals.append(v)

        ops = _as_list(deriv.get("operations"))
        for i, op in enumerate(ops):
            opname, raw = next(iter(op.items()))
            raw = raw or {}
            kind = OPS[opname]["kind"]
            if kind == "aggregate":
                continue  # applied during right-side reduction (R003)
            if kind == "window":
                vals = self._window(opname, raw, rows, vals, deriv)
                continue
            fn = SCALAR_OPS.get(opname)
            if fn is None:
                raise DerivationError(f"{opname} is registered but not implemented")
            out = []
            for row, v in zip(rows, vals):
                self.row_lookup = lambda n, r=row: r.out.get(n)
                engine_ctx = _Ctx(self.data, self.row_lookup)
                try:
                    out.append(fn(v, self._args(raw, row, deriv), engine_ctx))
                except Unmapped:
                    if "operation" not in exc:
                        raise DerivationError(
                            f"{name}: {opname} could not map {v!r} and no unmapped "
                            f"exception is declared (R008)"
                        )
                    out.append(exc["operation"][1]["default"])
            vals = out

        conv = []
        for v in vals:
            try:
                conv.append(convert(v, self.coltype[name]))
            except DerivationError:
                if "convert" not in exc:
                    raise
                conv.append(convert(exc["convert"][1]["default"], self.coltype[name]))
        return conv

    def _window(self, opname, raw, rows, vals, deriv):
        groups = {}
        gb = deriv.get("group_by") or []
        for i, row in enumerate(rows):
            groups.setdefault(tuple(row.out.get(k) for k in gb), []).append(i)
        out = [None] * len(rows)
        for idx in groups.values():
            sub = [rows[i] for i in idx]
            args = {}
            for key, spec in raw.items():
                if isinstance(spec, list):
                    args[f"{key}_per_row"] = [
                        [self._args(s, r, deriv) for s in spec] for r in sub
                    ]
                else:
                    args[f"{key}_per_row"] = [self._args(spec, r, deriv) for r in sub]
            res = WINDOW_OPS[opname](sub, args, _Ctx(self.data, self.row_lookup))
            for pos, i in enumerate(idx):
                out[i] = res[pos]
        return out

    # -- driver ---------------------------------------------------------

    def run(self):
        templates = self.spec.get("rows") or []
        if templates:
            for t in templates:
                driver = t.get("dataset", self.base)
                for rec in self.data[driver]:
                    if t.get("filter") and not evaluate_predicate(
                        t["filter"], _rec_lookup(rec, driver)
                    ):
                        continue
                    self.rows.append(Row(rec, driver, t))
        else:
            if not self.base:
                raise DerivationError("no rows entry and no base (R001)")
            for rec in self.data[self.base]:
                self.rows.append(Row(rec, self.base, None))

        # phase 1: row-template derivations, per template, dependency ordered
        for t in templates:
            sub = [r for r in self.rows if r.template is t]
            driver = t.get("dataset", self.base)
            for name in _toposort(
                t["derivations"], self.order, lambda d: self.deps(d, driver)
            ):
                for r, v in zip(sub, self.evaluate(name, t["derivations"][name], sub)):
                    r.out[name] = v

        # phase 2: column derivations across all rows
        coldefs = {c["name"]: c["derivation"] for c in self.columns if "derivation" in c}
        for name in _toposort(
            coldefs, self.order, lambda d: self.deps(d, self.base)
        ):
            for r, v in zip(self.rows, self.evaluate(name, coldefs[name], self.rows)):
                r.out[name] = v

        self.rows.sort(key=lambda r: tuple(_ord(r.out.get(k)) for k in self.keys))
        return [{c: serialize(r.out.get(c)) for c in self.order} for r in self.rows]

    def expected(self):
        exp = self.path.parent / "expected"
        target = next(exp.glob("*.csv"))
        with open(target) as fh:
            return [dict(r) for r in csv.DictReader(fh)]


class _Ctx:
    def __init__(self, data, row_lookup):
        self.data = data
        self.row_lookup = row_lookup


def _ord(v):
    """Sort key for R005 output ordering: missing last, numbers before text."""
    if v is None or v == "":
        return (2, 0.0, "")
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return (0, float(v), "")
    return (1, 0.0, str(v))


def _var(a):
    return a["source"] if isinstance(a, dict) and "source" in a else str(a)


def _rec_lookup(rec, ds):
    def look(name):
        return rec.get(name.split(".", 1)[1] if name.startswith(ds + ".") else name)

    return look


def _arg_deps(raw):
    out = set()
    if isinstance(raw, dict):
        if set(raw) == {"source"} and "." not in str(raw["source"]):
            out.add(raw["source"])
        else:
            for k, v in raw.items():
                if k == "when" and isinstance(v, str):
                    out |= {x for x in predicate_vars(v) if "." not in x}
                else:
                    out |= _arg_deps(v)
    elif isinstance(raw, list):
        for v in raw:
            out |= _arg_deps(v)
    return out


def _toposort(defs, order, deps_of):
    pos = {n: i for i, n in enumerate(order)}
    pending = dict(defs)
    done, out = set(), []
    while pending:
        ready = sorted(
            (n for n, d in pending.items() if not (deps_of(d) & set(pending) - {n})),
            key=lambda n: pos.get(n, 0),
        )
        if not ready:
            raise DerivationError(f"dependency cycle among {sorted(pending)} (R001)")
        for n in ready:
            out.append(n)
            done.add(n)
            del pending[n]
    return out


def run_spec(path):
    return Spec(path).run()
