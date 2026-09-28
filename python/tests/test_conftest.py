"""conftest behavior: the benchmark corpus must be the repository's own
``benchmarks/`` directory, and a missing corpus fails loudly.
"""

import os

import conftest


def test_bench_points_at_repo_benchmarks():
    assert os.path.basename(conftest.BENCH) == "benchmarks"
    assert os.path.isdir(conftest.BENCH)
    assert len(conftest.benchmark_dirs()) > 0


def test_benchmark_dirs_fails_when_corpus_absent(monkeypatch):
    # 2026-09-25: BENCH hardcoded a VM-local path; when that directory was
    # absent the suite collected a single skip instead of failing.
    monkeypatch.setattr(conftest, "BENCH", "/nonexistent-corpus-xyz")
    try:
        conftest.benchmark_dirs()
    except RuntimeError:
        pass
    else:
        raise AssertionError("expected RuntimeError for a missing corpus")
