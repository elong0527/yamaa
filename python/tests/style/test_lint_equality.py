"""The Rust lint reports exactly the Python style tool's findings, and its fix
writes exactly the tool's output, on every YAML file under benchmarks/ (each
benchmark specification, project file and environment file among them) and
on rearranged variants of each specification, project and environment file.
The style tests' own cases are compared as they run (conftest.py)."""

from pathlib import Path

import pytest

pytest.importorskip(
    "yamaa._native",
    reason="the compiled native extension is not installed",
    exc_type=ImportError,
)

from equality import assert_equal, variants  # noqa: E402

ROOT = Path(__file__).resolve().parents[3]
FILES = sorted(ROOT.glob("benchmarks/**/*.yaml"))
SOURCES = sorted(
    {
        *ROOT.glob("benchmarks/*/spec*.yaml"),
        *ROOT.glob("benchmarks/*/project.yaml"),
        *ROOT.glob("benchmarks/*/*/environment.yaml"),
    }
)


@pytest.mark.parametrize("path", FILES, ids=lambda path: str(path.relative_to(ROOT)))
def test_every_benchmark_file(path):
    assert_equal(path.read_bytes().decode("utf-8"))


@pytest.mark.parametrize("path", SOURCES, ids=lambda path: str(path.relative_to(ROOT)))
def test_rearranged_benchmark_files(path):
    for variant in variants(path.read_bytes().decode("utf-8")):
        assert_equal(variant)
