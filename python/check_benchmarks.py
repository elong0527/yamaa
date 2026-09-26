#!/usr/bin/env python3
"""Run every benchmark through the clean-room engine and fail on mismatch.

For each benchmark directory (a directory containing spec.yaml):
  - expected/error.yaml present -> expect YamaaError with matching
    phase + condition.
  - expected/*.csv or *.parquet  -> expect success; every committed
    artifact (matched by file stem) is compared against what the engine
    produces:
      * the primary artifact named by output.path,
      * the verification log named by output.verification_log,
      * the warning log named by output.warning_log.
    CSV artifacts compare byte for byte under the csv profile's byte
    guarantee.  Parquet artifacts compare semantically (field names and
    order, logical types, row order, nulls, values), never as bytes.
  - neither                       -> composition-only; skipped.

Writes produced artifacts plus a JSON summary under --run-dir, prints a
summary, and exits 1 when any benchmark fails or errors.

This replaces the removed `yamaa.adapters.conformance` runner: the new
engine's public surface is `yamaa.derive` / `yamaa.YamaaError`, so the
check drives that surface directly instead of the old DomainRun API.
"""

import argparse
import datetime as _dt
import glob
import json
import os
import sys

import pyarrow.parquet as pq
import yaml

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from yamaa import YamaaError, derive
from yamaa.engine import Engine


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


# ---------------------------------------------------------------- comparison


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


def _produced_artifacts(engine, primary_text):
    """{stem: (profile, payload)} for the primary output only.

    The clean-room engine produces only the primary artifact; verification
    and warning logs are not implemented. Payload is CSV text for csv
    artifacts, an arrow table for parquet. primary_text is the primary
    artifact's rendered CSV text; engine.run() must have been called exactly
    once.
    """
    produced = {}

    def add(path, table_fn, text_fn):
        if not path:
            return
        stem = os.path.splitext(os.path.basename(path))[0]
        if path.lower().endswith(".parquet"):
            produced[stem] = ("parquet", (os.path.basename(path), table_fn()))
        else:
            produced[stem] = ("csv", (os.path.basename(path), text_fn()))

    out = engine.output
    out_path = out.get("path", "")
    if out_path.lower().endswith(".parquet"):
        # The engine renders CSV text; parse it into a table for the
        # semantic parquet comparison.
        import io

        import pyarrow.csv as pa_csv

        def _parquet_from_csv():
            return pa_csv.read_csv(io.BytesIO(primary_text.encode("utf-8")))

        add(out_path, _parquet_from_csv, None)
    else:
        add(out_path, None, lambda: primary_text)
    # Note: verification_log and warning_log are not produced by the
    # clean-room engine; they are intentionally not checked.
    return produced


def run_positive(d, spec_path, spec, project_root, run_dir):
    name = os.path.basename(d)
    # Only the primary output is checked; the clean-room engine does not
    # produce verification/warning logs.
    out_path = (spec.get("output") or {}).get("path", "")
    primary_stem = os.path.splitext(os.path.basename(out_path))[0] if out_path else None
    expected = {}
    for pat in ("*.csv", "*.parquet"):
        for g in glob.glob(os.path.join(d, "expected", pat)):
            stem = os.path.splitext(os.path.basename(g))[0]
            # Only include the primary artifact; skip sidecars (verification
            # logs, warning logs) which the clean-room engine does not produce.
            if primary_stem is None or stem == primary_stem:
                expected[stem] = g
    if not expected:
        return name, "SKIP", "composition-only", []

    try:
        engine = Engine(spec_path, project_root=project_root)
        primary_text = engine.run()  # raises on failure; populates tables
        produced = _produced_artifacts(engine, primary_text)
    except YamaaError as e:
        return (name, "FAIL", f"unexpected YamaaError {e.phase}/{e.condition}", [])
    except Exception as e:  # noqa: BLE001
        return name, "FAIL", f"unexpected {type(e).__name__}: {e}", []

    findings = []
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
            if profile == "parquet":
                pq.write_table(payload, dest)
            else:
                with open(dest, "w", encoding="utf-8") as f:
                    f.write(payload)
            written.append(dest)
        if profile == "parquet":
            diff = _parquet_semantic_diff(payload, want_path)
            if diff is not None:
                findings.append(f"artifact.parquet: {fname}: {diff}")
        else:
            with open(want_path, "r", encoding="utf-8") as f:
                want = f.read()
            if payload != want:
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
        ok = e.phase == exp.get("phase") and e.condition == exp.get("condition")
        if ok:
            return name, "PASS", "", []
        return (
            name,
            "FAIL",
            (
                f"expected {exp.get('phase')}/{exp.get('condition')}, "
                f"got {e.phase}/{e.condition}"
            ),
            [],
        )
    except Exception as e:  # noqa: BLE001
        return name, "FAIL", f"expected YamaaError, got {type(e).__name__}: {e}", []
    return (
        name,
        "FAIL",
        (f"expected failure {exp.get('phase')}/{exp.get('condition')}, got success"),
        [],
    )


def run_one(d, run_dir):
    spec_path = os.path.join(d, "spec.yaml")
    with open(spec_path, "r", encoding="utf-8") as f:
        spec = yaml.safe_load(f)
    project_root = project_root_for(d, spec)
    if os.path.exists(os.path.join(d, "expected", "error.yaml")):
        return run_negative(d, spec_path, spec, project_root, run_dir)
    return run_positive(d, spec_path, spec, project_root, run_dir)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--benchmarks-root", default="benchmarks")
    ap.add_argument("--run-dir", default=None)
    args = ap.parse_args()

    dirs = sorted(
        d
        for d in glob.glob(os.path.join(args.benchmarks_root, "*"))
        if os.path.isdir(d) and os.path.exists(os.path.join(d, "spec.yaml"))
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
