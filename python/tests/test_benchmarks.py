"""Step-1 parity: every benchmark in the corpus runs through the clean-room
``derive()`` and matches its expected artifact (or expected error).

Negative benchmarks pin the (phase, condition) pair. Positive benchmarks
compare the derived CSV byte-for-byte against the golden artifact. The one
composition-only skip is honored here too.
"""

import glob
import os

import yaml

from yamaa import YamaaError, derive


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


def test_benchmark(bench_dir):
    name = os.path.basename(bench_dir)
    spec_path = os.path.join(bench_dir, "spec.yaml")
    with open(spec_path, "r", encoding="utf-8") as f:
        spec = yaml.safe_load(f)
    project_root = _project_root_for(bench_dir, spec)
    err_path = os.path.join(bench_dir, "expected", "error.yaml")
    goldens = sorted(glob.glob(os.path.join(bench_dir, "expected", "*.csv")))
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
            assert e.phase == exp.get("phase"), (
                f"{name}: phase {e.phase} != {exp.get('phase')}"
            )
            assert e.condition == exp.get("condition"), (
                f"{name}: condition {e.condition} != {exp.get('condition')}"
            )
        else:
            raise AssertionError(
                f"{name}: expected failure "
                f"{exp.get('phase')}/{exp.get('condition')}, got success"
            )
        return

    assert goldens, f"{name}: no golden artifact and no error.yaml"
    out_name = os.path.basename((spec.get("output") or {}).get("path", ""))
    golden = next((g for g in goldens if os.path.basename(g) == out_name), goldens[0])
    try:
        got = derive(spec_path, project_root=project_root)
    except YamaaError as e:
        raise AssertionError(
            f"{name}: unexpected YamaaError {e.phase}/{e.condition} "
            f"{e.requirement} {e.spec_paths}"
        ) from e
    with open(golden, "r", encoding="utf-8") as f:
        want = f.read()
    assert got == want, f"{name}: derived output differs from golden"
