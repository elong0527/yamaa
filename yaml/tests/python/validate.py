"""Conformance validation for derivation specifications.

Checks a spec against schema.yaml (R006), operations.yaml (R007),
exceptions.yaml (R008), and the structural clauses of R001, R002, R003 and R005.

This validates shape and vocabulary only. Executing a specification and
comparing its output to the expected CSV is engine.py.
"""
from __future__ import annotations

import re
from pathlib import Path

import yaml

from functions import HOST_FUNCTIONS
from predicates import DerivationError, check_syntax, predicate_vars

YAML_DIR = Path(__file__).resolve().parents[2]


def _load(name):
    with open(YAML_DIR / name) as fh:
        return yaml.safe_load(fh)


SCHEMA = _load("schema.yaml")
OPS = _load("operations.yaml")["operations"]
EXC = _load("exceptions.yaml")["exceptions"]
VARPAT = re.compile(SCHEMA["variable"]["pattern"])

# R006 scalar resolution: values PyYAML or the R yaml package resolve to boolean.
# Flagged in raw text because by parse time the information is already lost.
_BOOLISH = re.compile(
    r"^\s*(?:-\s*)?[A-Za-z_][\w.]*:\s+(Y|N|yes|no|on|off|Yes|No|On|Off|YES|NO)\s*$"
)

_SCALARS = {"bool": bool, "int": int, "str": str, "float": (int, float)}


def _fields(cls):
    return [next(iter(e)) for e in SCHEMA[cls]]


def _argspec(entry):
    return {next(iter(a)): next(iter(a.values())) for a in (entry.get("arguments") or [])}


def _as_list(v):
    if v is None:
        return []
    return v if isinstance(v, list) else [v]


def fold_ascii(s):
    """R007 case folding: A-Z to a-z, every other code point untouched."""
    return "".join(chr(ord(c) + 32) if "A" <= c <= "Z" else c for c in str(s))


def check_registries():
    """R007/R008 invariants on the registry documents themselves."""
    errs = []
    for label, reg in (("operation", OPS), ("exception", EXC)):
        for name, entry in reg.items():
            if not (entry.get("semantics") or "").strip():
                errs.append(f"registry: {label} {name!r} has no semantics")
            for arg, desc in _argspec(entry).items():
                if "default" not in desc:
                    continue
                if desc.get("required"):
                    errs.append(f"registry: {name}.{arg} has a default on a required argument")
                if "values" in desc and desc["default"] not in desc["values"]:
                    errs.append(
                        f"registry: {name}.{arg} default {desc['default']!r} "
                        f"is not in {desc['values']}"
                    )
                want = _SCALARS.get(desc.get("type"))
                if want and not isinstance(desc["default"], want):
                    errs.append(
                        f"registry: {name}.{arg} default {desc['default']!r} "
                        f"does not satisfy type {desc['type']!r}"
                    )
    return errs


class _Validator:
    def __init__(self, spec, raw):
        self.spec = spec
        self.raw = raw
        self.datasets = spec.get("datasets") or {}
        self.declared = {c["name"] for c in spec.get("columns") or []}
        self.errs = []

    def fail(self, msg):
        self.errs.append(msg)

    # ---- R006 -----------------------------------------------------------

    def closed(self, val, cls, where):
        allowed = set(_fields(cls))
        for key in val:
            if key not in allowed:
                self.fail(f"{where}: undeclared field {key!r} for {cls}")
        for entry in SCHEMA[cls]:
            name, desc = next(iter(entry.items()))
            if desc.get("required") and name not in val:
                self.fail(f"{where}: missing required field {name!r}")

    def scalars(self, path):
        for n, line in enumerate(self.raw.splitlines(), 1):
            m = _BOOLISH.match(line)
            if m:
                self.fail(
                    f"{path}:{n}: unquoted {m.group(1)!r} resolves as boolean in the R "
                    f"yaml package and/or PyYAML (R006 scalar resolution)"
                )

    # ---- R007 / R008 ----------------------------------------------------

    def action(self, act, where, registry, label):
        """action_class: a size-1 mapping of registered name to named arguments."""
        if not isinstance(act, dict) or len(act) != 1:
            self.fail(f"{where}: action_class requires size 1, got {act!r}")
            return None, None
        name, args = next(iter(act.items()))
        if name not in registry:
            self.fail(f"{where}: {name!r} is not in the {label} registry")
            return None, None
        entry = registry[name]
        args = args or {}
        spec = _argspec(entry)
        for arg in args:
            if arg not in spec:
                self.fail(
                    f"{where}.{name}: unknown argument {arg!r} "
                    f"(registered: {sorted(spec) or 'none'})"
                )
        for arg, desc in spec.items():
            if desc.get("required") and arg not in args:
                self.fail(f"{where}.{name}: missing required argument {arg!r}")
            if arg not in args:
                continue
            if "values" in desc and args[arg] not in desc["values"]:
                self.fail(
                    f"{where}.{name}.{arg}: {args[arg]!r} is not one of {desc['values']}"
                )
            if desc.get("type") == "dataset_id" and args[arg] not in self.datasets:
                self.fail(
                    f"{where}.{name}.{arg}: {args[arg]!r} is not a declared dataset "
                    f"(declared: {sorted(self.datasets)})"
                )
        for arg, desc in spec.items():
            if desc.get("type") != "sql" or arg not in args:
                continue
            self.predicate(args[arg], f"{where}.{name}", arg)
        for arg in args:
            for item in (args[arg] if isinstance(args[arg], list) else []):
                if isinstance(item, dict) and isinstance(item.get("when"), str):
                    self.predicate(item["when"], f"{where}.{name}.{arg}", "when")
        for arg, val in args.items():
            self.no_nested_action(val, f"{where}.{name}.{arg}")
        if name == "call" and isinstance(args.get("function"), str):
            if args["function"] not in HOST_FUNCTIONS:
                self.fail(
                    f"{where}.call.function: {args['function']!r} is not in the host "
                    f"function library (available: {sorted(HOST_FUNCTIONS)}) (R007)"
                )
        if name == "mapping" and args.get("case_sensitive") is False:
            seen = {}
            for key in args.get("dict") or {}:
                folded = fold_ascii(key)
                if folded in seen:
                    self.fail(
                        f"{where}.mapping.dict: keys {seen[folded]!r} and {key!r} fold "
                        f"to the same value under case_sensitive: false"
                    )
                seen[folded] = key
        return name, entry

    def no_nested_action(self, val, where):
        """An argument holds values, never an operation. R007: `action_argument`
        has no `action_class` member, so a nested operation is a silent no-op
        that lands the mapping itself in the output."""
        if isinstance(val, dict):
            if len(val) == 1:
                only = next(iter(val))
                if only != "source" and (only in OPS or only in EXC):
                    self.fail(
                        f"{where}: {only!r} is an operation nested where a value is "
                        f"expected; arguments cannot hold operations (R007)"
                    )
            for k, sub in val.items():
                self.no_nested_action(sub, f"{where}.{k}")
        elif isinstance(val, list):
            for i, sub in enumerate(val):
                self.no_nested_action(sub, f"{where}[{i}]")

    # ---- derivations ----------------------------------------------------

    def predicate(self, text, where, label):
        """R001: a predicate may only name declared output columns."""
        try:
            check_syntax(text)
            names = predicate_vars(text)
        except DerivationError as exc:
            self.fail(f"{where}.{label}: {exc}")
            return
        for n in names:
            if "." in n:
                self.fail(f"{where}.{label}: {n!r} is qualified; a predicate over "
                          f"output rows names output columns (R001)")
            elif n not in self.declared:
                self.fail(f"{where}.{label}: {n!r} is not a declared output column (R001)")

    def derivation(self, d, where, driver):
        self.closed(d, "derivation_class", where)
        if d.get("where"):
            self.predicate(d["where"], where, "where")

        src = d.get("source")
        if src is not None and not VARPAT.match(str(src)):
            self.fail(f"{where}.source: {src!r} fails the variable pattern")
        qualifier = str(src).split(".")[0] if src and "." in str(src) else None
        if qualifier is not None and qualifier not in self.datasets:
            self.fail(
                f"{where}.source: {qualifier!r} is not a declared dataset "
                f"(declared: {sorted(self.datasets)}) (R002)"
            )

        joins = qualifier is not None and qualifier != driver and qualifier in self.datasets
        if d.get("filter") is not None and not isinstance(d["filter"], str):
            self.fail(f"{where}.filter: must be one sql predicate (R004)")
        if "filter" in d and not joins:
            self.fail(f"{where}.filter: derivation performs no join (R003)")

        ops = _as_list(d.get("operations"))
        resolved = []
        for i, op in enumerate(ops):
            name, entry = self.action(
                op, f"{where}.operations[{i}]", OPS, "operation"
            )
            resolved.append((name, entry))
            if entry and entry["kind"] == "aggregate" and not (
                (i == 0 and joins) or "group_by" in d
            ):
                self.fail(
                    f"{where}.operations[{i}].{name}: aggregate outside its two legal "
                    f"positions -- not a leading right-side reduction, no group_by (R007)"
                )

        seeded = "source" in d or "literal" in d
        if ops and resolved[0][1] and resolved[0][1]["seed"] == "required" and not seeded:
            self.fail(
                f"{where}: leading {resolved[0][0]!r} declares seed: required but the "
                f"derivation has no source or literal (R007)"
            )
        if not ops and not seeded:
            self.fail(f"{where}: no source, literal, or operations (R004)")
        if "source" in d and "literal" in d:
            self.fail(f"{where}: both source and literal (R004)")

        stages = {}
        for i, ex in enumerate(_as_list(d.get("exception"))):
            name, entry = self.action(
                ex, f"{where}.exception[{i}]", EXC, "exception"
            )
            if not entry:
                continue
            stage = entry["stage"]
            if stage in stages:
                self.fail(
                    f"{where}.exception[{i}].{name}: stage {stage!r} already bound by "
                    f"{stages[stage]!r} (R008 permits one per stage)"
                )
            stages[stage] = name
            if stage == "operation":
                n = sum(1 for _, e in resolved if e and name in (e.get("raises") or []))
                if n != 1:
                    self.fail(
                        f"{where}.exception[{i}].{name}: bound to {n} operations that "
                        f"raise it, requires exactly 1 (R008)"
                    )

    # ---- entry point ----------------------------------------------------

    def run(self, path):
        self.scalars(path)
        self.closed(self.spec, "root_class", "root")

        domain = self.spec.get("domain")
        if domain in self.datasets:
            self.fail(
                f"root.datasets: {domain!r} is also the output domain; a source dataset "
                f"must not reuse the domain name (R002)"
            )

        rows = self.spec.get("rows") or []
        base = self.spec.get("base")
        if not rows and base is None:
            self.fail("root: no rows entry and no base (R001)")
        if base is not None and base not in self.datasets:
            self.fail(f"root.base: {base!r} is not a declared dataset (R002)")

        declared = {c["name"] for c in self.spec.get("columns", [])}
        for col in self.spec.get("columns", []):
            self.closed(col, "column_class", f"columns[{col.get('name')}]")
            if "derivation" in col:
                self.derivation(
                    col["derivation"], f"columns[{col['name']}].derivation", base
                )

        for row in rows:
            rid = row.get("id")
            self.closed(row, "row_class", f"rows[{rid}]")
            if row.get("filter") is not None and not isinstance(row["filter"], str):
                self.fail(f"rows[{rid}].filter: must be one sql predicate (R004)")
            if "dataset" in row and row["dataset"] not in self.datasets:
                self.fail(f"rows[{rid}].dataset: {row['dataset']!r} is not declared (R002)")
            for name, d in (row.get("derivations") or {}).items():
                if name not in declared:
                    self.fail(f"rows[{rid}]: target {name!r} is not a declared column")
                self.derivation(d, f"rows[{rid}].{name}", row.get("dataset", base))

        col_derived = {
            c["name"] for c in self.spec.get("columns", []) if "derivation" in c
        }
        row_targets = [set(r.get("derivations") or {}) for r in rows]
        for name in sorted(declared):
            covered = name in col_derived or (
                row_targets and all(name in t for t in row_targets)
            )
            if not covered:
                self.fail(
                    f"R005 coverage: {name!r} has no column derivation and is missing "
                    f"from at least one rows entry"
                )
        for key in self.spec.get("keys", []):
            if key not in declared:
                self.fail(f"R005: key {key!r} is not a declared column")

        return self.errs


def validate_spec(path):
    """Return a list of conformance errors for one specification file."""
    path = Path(path)
    raw = path.read_text()
    return _Validator(yaml.safe_load(raw), raw).run(str(path))


def example_specs():
    return sorted((YAML_DIR / "examples").glob("*/spec.yaml"))


if __name__ == "__main__":
    import sys

    failed = check_registries()
    for err in failed:
        print(f"FAIL {err}")
    for spec in example_specs():
        errs = validate_spec(spec)
        print(f"{'FAIL' if errs else 'ok  '} {spec.parent.name}")
        for err in errs:
            print(f"       {err}")
        failed += errs
    sys.exit(1 if failed else 0)
