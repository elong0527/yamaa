"""Regression tests for the yamaa.runtime <-> yamaa.planning circular import."""

from __future__ import annotations

import subprocess
import sys

import pytest


@pytest.mark.parametrize(
    "statement",
    [
        pytest.param("import yamaa.runtime", id="runtime-first"),
        pytest.param("import yamaa.planning", id="planning-first"),
        pytest.param(
            "import yamaa.runtime; import yamaa.planning", id="runtime-then-planning"
        ),
        pytest.param(
            "import yamaa.planning; import yamaa.runtime", id="planning-then-runtime"
        ),
    ],
)
def test_clean_interpreter_import_order(statement: str) -> None:
    """A fresh interpreter must import yamaa.runtime and yamaa.planning in any order.

    Regression test: importing yamaa.runtime.executor at module level from
    yamaa.planning.workflow re-entered the partially initialized executor
    module whenever yamaa.runtime was imported before yamaa.planning.
    """
    subprocess.run(
        [sys.executable, "-c", statement],
        check=True,
        capture_output=True,
        text=True,
        timeout=120,
    )
