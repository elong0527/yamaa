#!/usr/bin/env python3
"""Run every benchmark through the clean-room engine and fail on mismatch.

Every directory under the benchmarks root is a benchmark. It runs
`spec.yaml`, or else the `spec_*.yaml` files `benchmark_specs` selects
(benchmarks/agents.md): each producer an entry reads through
`input.<id>.schema`, then the entry, the file no other file names as a
parent or a producer. A directory with no specification, or whose files
name each other so that none is the entry, fails.

For each specification run:
  - expected/error.yaml present -> expect YamaaError matching the full
    pin: phase, condition, requirement, spec_paths, and every key the pin
    lists under context (the engine may report more keys).
  - expected/*.csv or *.parquet  -> expect success; every committed
    artifact (matched by file stem) is compared against what the engine
    produces:
      * the primary artifact named by output.path,
      * the verification log named by output.verification_log,
      * the warning log named by output.warning_log.
    CSV artifacts compare byte for byte under the csv profile's byte
    guarantee.  Parquet artifacts compare semantically (field names and
    order, logical types, row order, nulls, values), never as bytes.
    An entry that names `parents` also compares its resolved YAML data
    tree with expected/spec_resolved.yaml (multi-level) or
    expected/resolved[_<variant>].yaml (REQ-0650).
  - neither                       -> no expected artifact; fails.

Writes produced artifacts plus a JSON summary under --run-dir, prints a
summary, and exits 1 when any benchmark fails or errors.

This replaces the removed `yamaa.adapters.conformance` runner: the new
engine's public surface is `yamaa.derive` / `yamaa.derive_artifacts` /
`yamaa.YamaaError`, so the check drives that surface directly instead of
the old DomainRun API.
"""

import argparse
import datetime as _dt
import glob
import json
import os
import sys

import pyarrow as pa
import pyarrow.parquet as pq
import yaml

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from yamaa import YamaaError, derive, derive_artifacts
from yamaa.compose import resolve


def walk(o):
    if isinstance(o, dict):
        for k, v in o.items():
            yield k, v
            yield from walk(v)
    elif isinstance(o, list):
        for v in o:
            yield from walk(v)


def uses_function(spec):
    return any(k == "function" for k, _ in walk(spec))


def project_root_for(d, spec):
    """Directory containing environment.yaml for function benchmarks."""
    if not uses_function(spec):
        return None
    py_dir = os.path.join(d, "python")
    if os.path.isfile(os.path.join(py_dir, "environment.yaml")):
        return py_dir
    return d


def _load_spec(path):
    with open(path, "r", encoding="utf-8") as f:
        doc = yaml.safe_load(f)
    return doc if isinstance(doc, dict) else {}


def benchmark_specs(d):
    """The specifications a benchmark runs, in run order, the entry last.

    benchmarks/agents.md: `spec.yaml` is the one specification. Without it,
    each `spec_*.yaml` is an inheritance level, a producer another file
    reads through `input.<id>.schema`, or an entry, the file no other file
    names as a parent or a producer. A level runs only as part of its entry;
    a producer runs on its own, before the entry that reads it, and commits
    its own artifact. Raises ValueError when there is nothing to run.
    """
    if os.path.isfile(os.path.join(d, "spec.yaml")):
        return [os.path.join(d, "spec.yaml")]
    specs = {os.path.realpath(p): p for p in glob.glob(os.path.join(d, "spec_*.yaml"))}
    if not specs:
        raise ValueError("no spec.yaml and no spec_*.yaml")
    parents, producers = {}, {}
    for real, path in specs.items():
        doc = _load_spec(path)
        named = doc.get("parents") or []
        inputs = doc.get("input") if isinstance(doc.get("input"), dict) else {}
        schemas = [s.get("schema") for s in inputs.values() if isinstance(s, dict)]
        near = os.path.dirname(real)
        parents[real], producers[real] = (
            [
                os.path.realpath(os.path.join(near, n))
                for n in ([names] if isinstance(names, str) else names)
                if isinstance(n, str)
            ]
            for names in (named, schemas)
        )
    named = {p for names in (*parents.values(), *producers.values()) for p in names}
    entries = sorted((p for p in specs if p not in named), key=lambda p: specs[p])
    if not entries:
        raise ValueError("every spec_*.yaml is named by another, so none is the entry")
    order = []

    def visit(real, chain):
        # A producer that any level of the chain reads runs first.
        levels = [real]
        for level in levels:
            levels.extend(p for p in parents.get(level, []) if p not in levels)
        for level in levels:
            for producer in producers.get(level, []):
                if producer in specs and producer not in order + chain:
                    visit(producer, chain + [producer])
        if real not in order:
            order.append(real)

    for entry in entries:
        visit(entry, [entry])
    return [specs[p] for p in order]


def resolved_fixture(d, spec_path):
    """benchmarks/agents.md: a multi-level benchmark commits
    expected/spec_resolved.yaml; `spec[_<variant>].yaml` commits
    expected/resolved[_<variant>].yaml."""
    if not os.path.isfile(os.path.join(d, "spec.yaml")):
        return os.path.join(d, "expected", "spec_resolved.yaml")
    stem = os.path.splitext(os.path.basename(spec_path))[0]
    return os.path.join(d, "expected", f"resolved{stem[len('spec') :]}.yaml")


def resolved_finding(d, spec_path, spec):
    """REQ-0650: compare a composed entry's resolved YAML data tree."""
    if not spec.get("parents"):
        return None
    fixture = resolved_fixture(d, spec_path)
    if not os.path.isfile(fixture):
        return f"resolved.missing: expected/{os.path.basename(fixture)} not committed"
    try:
        got = resolve(spec_path, spec).document
    except YamaaError as e:
        return f"resolved: unexpected YamaaError {e.phase}/{e.condition}"
    with open(fixture, "r", encoding="utf-8") as f:
        want = yaml.safe_load(f)
    if got != want:
        return f"resolved: expected/{os.path.basename(fixture)} differs"
    return None


# ---------------------------------------------------------------- comparison

# The pinned fields compared exactly; `context` is compared key by key.
PIN_FIELDS = ("phase", "condition", "requirement", "spec_paths")


def pin_mismatches(err, pin):
    """How a raised YamaaError differs from its expected/error.yaml pin.

    Phase and condition are always compared, requirement and spec_paths
    whenever pinned, and each key the pin lists under `context`; context
    keys the pin does not list are the engine's own detail and are not
    compared. Returns an empty list when the error matches its pin.
    """
    got = {
        "phase": err.phase,
        "condition": err.condition,
        "requirement": err.requirement,
        "spec_paths": list(err.spec_paths),
    }
    out = [
        f"{field} {got[field]!r} != pinned {pin.get(field)!r}"
        for field in PIN_FIELDS
        if (field in pin or field in ("phase", "condition"))
        and got[field] != pin.get(field)
    ]
    context = err.context or {}
    for key, want in (pin.get("context") or {}).items():
        if key not in context:
            out.append(f"context.{key} missing, pinned {want!r}")
        elif context[key] != want:
            out.append(f"context.{key} {context[key]!r} != pinned {want!r}")
    return out


def _csv_record(fields):
    """R020 quoting: quote fields containing '"', ',', CR, LF."""
    out = []
    for f in fields:
        if any(c in f for c in '",\r\n'):
            out.append('"' + f.replace('"', '""') + '"')
        else:
            out.append(f)
    return ",".join(out)


def _table_to_csv_text(table):
    """Render an arrow table as CSV text under the csv profile."""
    cols = table.schema.names
    lines = [_csv_record(cols)]
    for row in table.to_pylist():
        lines.append(_csv_record(["" if row[c] is None else str(row[c]) for c in cols]))
    return "".join(line + "\n" for line in lines)


def _parquet_canonical(value):
    """Canonical value form for the semantic parquet comparison."""
    if value is None:
        return None
    if isinstance(value, float):
        return repr(value)  # shortest round-trip text, like json.dumps
    if isinstance(value, (_dt.date, _dt.datetime)):
        return value.isoformat()
    if isinstance(value, (bytes, bytearray)):
        return bytes(value).hex()
    if isinstance(value, (str, int, bool)):
        return value
    return str(value)


def _parquet_logical(table):
    """(field names, logical types, canonical rows) of an arrow table."""
    names = table.schema.names
    types = [str(table.schema.field(n).type) for n in names]
    pylist = table.to_pylist()
    rows = [[_parquet_canonical(row[n]) for n in names] for row in pylist]
    return names, types, rows


def _parquet_semantic_diff(got_table, want_path):
    """Diff two parquet artifacts semantically, never as bytes.

    REQ-0740: field names in order, logical types, row order, nulls, values.
    Returns None when they agree, else a human-readable diff.
    """
    want = pq.read_table(want_path)
    want_names, want_types, want_rows = _parquet_logical(want)
    got_names, got_types, got_rows = _parquet_logical(got_table)
    if got_names != want_names:
        return f"field names differ: {got_names} vs {want_names}"
    if got_types != want_types:
        return f"logical types differ: {got_types} vs {want_types}"
    if len(got_rows) != len(want_rows):
        return f"row count differs: {len(got_rows)} vs {len(want_rows)}"
    for i, (g, w) in enumerate(zip(got_rows, want_rows)):
        if g != w:
            for name, a, b in zip(got_names, g, w):
                if a != b:
                    return f"column {name} row {i} differs: {a!r} vs {b!r}"
            return f"row {i} differs"
    return None


# ---------------------------------------------------------------- execution


def _produced_artifacts(artifacts):
    """{stem: (profile, (file name, payload))} for every file the run
    publishes: the primary artifact and the declared verification and
    warning logs. Payload is CSV text for csv artifacts and the written
    bytes for parquet."""
    produced = {}
    for path, payload in artifacts.items():
        fname = os.path.basename(path)
        profile = "parquet" if path.lower().endswith(".parquet") else "csv"
        produced[os.path.splitext(fname)[0]] = (profile, (fname, payload))
    return produced


def run_positive(d, specs, run_dir):
    """Run each spec in order, producers before their entry, and compare
    every artifact they publish together, the primary outputs and their
    logs, with the committed ones."""
    name = os.path.basename(d)
    expected = {}
    for pat in ("*.csv", "*.parquet"):
        for g in glob.glob(os.path.join(d, "expected", pat)):
            expected[os.path.splitext(os.path.basename(g))[0]] = g
    if not expected:
        return name, "FAIL", "artifact.missing: no expected artifact", []

    findings = []
    produced = {}
    for spec_path in specs:
        spec = _load_spec(spec_path)
        label = f"{os.path.basename(spec_path)}: " if len(specs) > 1 else ""
        finding = resolved_finding(d, spec_path, spec)
        if finding is not None:
            findings.append(label + finding)
        try:
            got = _produced_artifacts(
                derive_artifacts(spec_path, project_root=project_root_for(d, spec))
            )
        except YamaaError as e:
            findings.append(f"{label}unexpected YamaaError {e.phase}/{e.condition}")
            continue
        except Exception as e:  # noqa: BLE001
            findings.append(f"{label}unexpected {type(e).__name__}: {e}")
            continue
        for stem, artifact in got.items():
            if stem in produced:
                findings.append(f"artifact.duplicate: {stem} published twice")
            produced[stem] = artifact
    if findings:
        return name, "FAIL", "; ".join(findings), []

    written = []
    for stem in sorted(set(produced) | set(expected)):
        if stem not in produced:
            findings.append(f"artifact.missing: {stem} not produced")
            continue
        if stem not in expected:
            findings.append(f"artifact.unexpected: {stem} not committed")
            continue
        profile, (fname, payload) = produced[stem]
        want_path = expected[stem]
        if run_dir:
            dest = os.path.join(run_dir, "artifacts", name, fname)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(dest, "wb") as f:
                f.write(payload if profile == "parquet" else payload.encode("utf-8"))
            written.append(dest)
        if profile == "parquet":
            # The direct reader: pq.read_table over a buffer can deadlock
            # pyarrow's thread pool at interpreter exit.
            got = pq.ParquetFile(pa.BufferReader(payload)).read()
            diff = _parquet_semantic_diff(got, want_path)
            if diff is not None:
                findings.append(f"artifact.parquet: {fname}: {diff}")
        else:
            with open(want_path, "rb") as f:
                want = f.read()
            if payload.encode("utf-8") != want:
                findings.append(f"artifact.csv: {fname}: rendered bytes differ")
    if findings:
        return name, "FAIL", "; ".join(findings), written
    return name, "PASS", "", written


def run_negative(d, spec_path, spec, project_root, run_dir):
    name = os.path.basename(d)
    with open(os.path.join(d, "expected", "error.yaml"), "r", encoding="utf-8") as f:
        exp = yaml.safe_load(f)
    try:
        derive(spec_path, project_root=project_root)
    except YamaaError as e:
        mismatches = pin_mismatches(e, exp)
        if not mismatches:
            return name, "PASS", "", []
        return name, "FAIL", "; ".join(mismatches), []
    except Exception as e:  # noqa: BLE001
        return name, "FAIL", f"expected YamaaError, got {type(e).__name__}: {e}", []
    return (
        name,
        "FAIL",
        (f"expected failure {exp.get('phase')}/{exp.get('condition')}, got success"),
        [],
    )


def run_one(d, run_dir):
    name = os.path.basename(d)
    specs = benchmark_specs(d)
    if os.path.exists(os.path.join(d, "expected", "error.yaml")):
        if len(specs) != 1:
            names = [os.path.basename(s) for s in specs]
            return name, "ERROR", f"runner: a negative runs one entry, got {names}", []
        spec = _load_spec(specs[0])
        return run_negative(d, specs[0], spec, project_root_for(d, spec), run_dir)
    return run_positive(d, specs, run_dir)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--benchmarks-root", default="benchmarks")
    ap.add_argument("--run-dir", default=None)
    args = ap.parse_args()

    dirs = sorted(
        d
        for d in glob.glob(os.path.join(args.benchmarks_root, "*"))
        if os.path.isdir(d)
    )
    results = []
    for d in dirs:
        try:
            results.append(run_one(d, args.run_dir))
        except Exception as e:  # noqa: BLE001 - runner must not die
            results.append(
                (os.path.basename(d), "ERROR", f"runner: {type(e).__name__}: {e}", [])
            )
    npass = sum(1 for r in results if r[1] == "PASS")
    nfail = sum(1 for r in results if r[1] == "FAIL")
    nskip = sum(1 for r in results if r[1] == "SKIP")
    nerr = sum(1 for r in results if r[1] == "ERROR")
    for name, status, detail, _written in results:
        if status in ("FAIL", "ERROR"):
            print(f"{status:5} {name} :: {detail}")
    print(
        f"\n{len(results)} benchmarks: {npass} PASS, {nfail} FAIL, "
        f"{nskip} SKIP, {nerr} ERROR"
    )
    if args.run_dir:
        os.makedirs(os.path.join(args.run_dir, "reports"), exist_ok=True)
        summary = os.path.join(args.run_dir, "reports", "summary.json")
        with open(summary, "w", encoding="utf-8") as f:
            json.dump(
                [{"name": n, "status": s, "detail": d} for n, s, d, _w in results],
                f,
                indent=1,
            )
        with open(
            os.path.join(args.run_dir, "reports", "failures.txt"), "w", encoding="utf-8"
        ) as f:
            for n, s, d, _w in results:
                if s in ("FAIL", "ERROR"):
                    f.write(f"{s} {n} :: {d}\n")
    sys.exit(1 if (nfail or nerr) else 0)


if __name__ == "__main__":
    main()
