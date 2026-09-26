"""Shared fixtures for the replacement-engine test suite.

Puts the replacement ``src`` tree (``python/src``) on ``sys.path`` so the
tests measure the replacement ``yamaa`` package itself, never an installed
copy.
"""

import glob
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "src")
# The corpus under test is the repository's own benchmarks/ directory, so the
# suite runs the same anywhere the repo is checked out.
BENCH = os.path.join(os.path.dirname(ROOT), "benchmarks")

if SRC not in sys.path:
    sys.path.insert(0, SRC)


def benchmark_dirs():
    return sorted(
        d
        for d in glob.glob(os.path.join(BENCH, "*"))
        if os.path.isdir(d) and os.path.isfile(os.path.join(d, "spec.yaml"))
    )


def pytest_generate_tests(metafunc):
    if "bench_dir" in metafunc.fixturenames:
        metafunc.parametrize(
            "bench_dir",
            benchmark_dirs(),
            ids=[os.path.basename(d) for d in benchmark_dirs()],
        )
