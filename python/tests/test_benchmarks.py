"""Step-1 parity: every benchmark in the corpus runs through the clean-room
engine and matches its expected artifacts (or expected error).

Negative benchmarks must match their full pin: phase, condition,
requirement, spec_paths, and every key the pin lists under context (the
engine may report more keys). Positive benchmarks compare every committed
artifact, the output and its warning and verification logs, against what
``derive_artifacts()`` publishes: csv byte for byte, parquet value for
value. The one composition-only skip is honored here too.
"""

import glob
import os

import pyarrow as pa
import pyarrow.parquet as pq
import yaml

from yamaa import YamaaError, derive, derive_artifacts


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


def test_benchmark(bench_dir):
    name = os.path.basename(bench_dir)
    spec_path = os.path.join(bench_dir, "spec.yaml")
    with open(spec_path, "r", encoding="utf-8") as f:
        spec = yaml.safe_load(f)
    project_root = _project_root_for(bench_dir, spec)
    err_path = os.path.join(bench_dir, "expected", "error.yaml")
    goldens = sorted(
        g
        for pattern in ("*.csv", "*.parquet")
        for g in glob.glob(os.path.join(bench_dir, "expected", pattern))
    )
    if not os.path.exists(err_path) and not goldens:
        # The corpus's single composition-only benchmark documents spec
        # composition without a runnable artifact.
        import pytest

        pytest.skip("composition-only benchmark")

    if os.path.exists(err_path):
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
    try:
        got = derive_artifacts(spec_path, project_root=project_root)
    except YamaaError as e:
        raise AssertionError(
            f"{name}: unexpected YamaaError {e.phase}/{e.condition} "
            f"{e.requirement} {e.spec_paths}"
        ) from e
    # Every committed artifact, the primary output and its declared logs, is
    # produced and matches: csv byte for byte, parquet value for value.
    produced = {os.path.basename(path): data for path, data in got.items()}
    committed = {os.path.basename(g): g for g in goldens}
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
