"""Step-1 parity: every benchmark in the corpus runs through the clean-room
engine and matches its expected artifacts (or expected error).

Every benchmark directory runs ``spec.yaml``, or else the ``spec_*.yaml``
files ``_benchmark_specs`` selects under benchmarks/agents.md: each producer
the entry reads, then the entry. Negative benchmarks must match their full
pin: phase, condition, requirement, spec_paths, and every key the pin lists
under context (the engine may report more keys). Positive benchmarks compare
every committed artifact, each spec's output and its warning and verification
logs, against what ``derive_artifacts()`` publishes: csv byte for byte,
parquet value for value. A composed entry's resolved specification is also
compared against its committed YAML data tree.
"""

import glob
import os

import pyarrow as pa
import pyarrow.parquet as pq
import yaml

from yamaa import YamaaError, derive, derive_artifacts
from yamaa.compose import resolve


def _walk(o):
    if isinstance(o, dict):
        for k, v in o.items():
            yield k, v
            yield from _walk(v)
    elif isinstance(o, list):
        for v in o:
            yield from _walk(v)


def _project_root_for(bench_dir, spec):
    """Directory containing environment.yaml for function benchmarks."""
    if not any(k == "function" for k, _ in _walk(spec)):
        return None
    py_dir = os.path.join(bench_dir, "python")
    if os.path.isfile(os.path.join(py_dir, "environment.yaml")):
        return py_dir
    return bench_dir


# The pinned fields compared exactly; `context` is compared key by key.
PIN_FIELDS = ("phase", "condition", "requirement", "spec_paths")


def _pin_mismatches(err, pin):
    """How a raised YamaaError differs from its expected/error.yaml pin:
    phase and condition always, requirement and spec_paths whenever
    pinned, and each context key the pin lists."""
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


def _load_spec(path):
    with open(path, "r", encoding="utf-8") as f:
        doc = yaml.safe_load(f)
    return doc if isinstance(doc, dict) else {}


def _benchmark_specs(bench_dir):
    """The specifications a benchmark runs, in run order, the entry last.

    benchmarks/agents.md: `spec.yaml` is the one specification. Without it,
    each `spec_*.yaml` is an inheritance level, a producer another file
    reads through `input.<id>.schema`, or the entry, the file no other file
    names as a parent or a producer. A producer runs before the entry that
    reads it and commits its own artifact; a level runs only in its entry.
    """
    single = os.path.join(bench_dir, "spec.yaml")
    if os.path.isfile(single):
        return [single]
    specs = {
        os.path.realpath(p): p
        for p in glob.glob(os.path.join(bench_dir, "spec_*.yaml"))
    }
    assert specs, f"{os.path.basename(bench_dir)}: no spec.yaml and no spec_*.yaml"
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
    assert entries, f"{os.path.basename(bench_dir)}: every spec_*.yaml is named"
    order = []

    def visit(real, chain):
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


def _resolved_fixture(bench_dir, spec_path):
    """expected/spec_resolved.yaml for a multi-level benchmark, else
    expected/resolved[_<variant>].yaml for spec[_<variant>].yaml."""
    if not os.path.isfile(os.path.join(bench_dir, "spec.yaml")):
        return os.path.join(bench_dir, "expected", "spec_resolved.yaml")
    stem = os.path.splitext(os.path.basename(spec_path))[0]
    return os.path.join(bench_dir, "expected", f"resolved{stem[len('spec') :]}.yaml")


def test_benchmark(bench_dir):
    name = os.path.basename(bench_dir)
    specs = _benchmark_specs(bench_dir)
    err_path = os.path.join(bench_dir, "expected", "error.yaml")
    goldens = sorted(
        g
        for pattern in ("*.csv", "*.parquet")
        for g in glob.glob(os.path.join(bench_dir, "expected", pattern))
    )
    if os.path.exists(err_path):
        assert len(specs) == 1, f"{name}: a negative runs one entry, got {specs}"
        spec_path = specs[0]
        spec = _load_spec(spec_path)
        project_root = _project_root_for(bench_dir, spec)
        with open(err_path, "r", encoding="utf-8") as f:
            exp = yaml.safe_load(f)
        try:
            derive(spec_path, project_root=project_root)
        except YamaaError as e:
            mismatches = _pin_mismatches(e, exp)
            assert not mismatches, f"{name}: " + "; ".join(mismatches)
        else:
            raise AssertionError(
                f"{name}: expected failure "
                f"{exp.get('phase')}/{exp.get('condition')}, got success"
            )
        return

    assert goldens, f"{name}: no golden artifact and no error.yaml"
    committed = {os.path.basename(g): g for g in goldens}
    produced = {}
    for spec_path in specs:
        spec = _load_spec(spec_path)
        project_root = _project_root_for(bench_dir, spec)
        label = name if len(specs) == 1 else f"{name}/{os.path.basename(spec_path)}"
        if spec.get("parents"):
            # REQ-0650: conformance compares the resolved YAML data tree.
            fixture = _resolved_fixture(bench_dir, spec_path)
            assert os.path.isfile(fixture), f"{label}: no {fixture}"
            with open(fixture, "r", encoding="utf-8") as f:
                want = yaml.safe_load(f)
            resolved = resolve(spec_path, spec).document
            assert resolved == want, f"{label}: resolved spec differs from {fixture}"
        try:
            got = derive_artifacts(spec_path, project_root=project_root)
        except YamaaError as e:
            raise AssertionError(
                f"{label}: unexpected YamaaError {e.phase}/{e.condition} "
                f"{e.requirement} {e.spec_paths}"
            ) from e
        for path, data in got.items():
            fname = os.path.basename(path)
            assert fname not in produced, f"{label}: {fname} published twice"
            produced[fname] = data
    # Every committed artifact, each spec's output and its declared logs, is
    # produced and matches: csv byte for byte, parquet value for value.
    assert sorted(produced) == sorted(committed), (
        f"{name}: produced {sorted(produced)}, committed {sorted(committed)}"
    )
    for fname, golden in committed.items():
        with open(golden, "rb") as f:
            want = f.read()
        if fname.lower().endswith(".parquet"):
            same = _parquet_values(produced[fname]) == _parquet_values(want)
        else:
            same = produced[fname].encode("utf-8") == want
        assert same, f"{name}: derived {fname} differs from golden"


def _parquet_values(data):
    """What REQ-0740 fixes of a parquet file rather than its bytes: field
    names in order, logical types, and every row's values (a float by its
    shortest round-trip text, so the comparison is bit-exact)."""
    # The direct reader: pq.read_table over a buffer can deadlock pyarrow's
    # thread pool at interpreter exit.
    table = pq.ParquetFile(pa.BufferReader(data)).read()
    rows = [[repr(v) for v in row.values()] for row in table.to_pylist()]
    return table.schema.names, [str(f.type) for f in table.schema], rows
