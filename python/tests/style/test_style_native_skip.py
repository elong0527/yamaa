"""Regression test for the native-extension skip in test_style.py."""

from __future__ import annotations

import subprocess
import sys
import textwrap
from pathlib import Path

STYLE_TEST = Path(__file__).with_name("test_style.py")


def test_style_tests_skip_when_native_extension_is_absent() -> None:
    """test_style.py must skip, not error, when yamaa._native is missing.

    The guard under test runs at import/collection time in test_style.py
    itself, so it cannot be exercised from inside that module (the module
    would already have been skipped). Run it in a clean interpreter with
    the native extension blocked and require a pytest skip.
    """
    script = textwrap.dedent(
        """
        import importlib.util
        import sys


        class _NativeBlocker:
            def find_spec(self, fullname, path=None, target=None):
                if fullname == "yamaa._native":
                    raise ModuleNotFoundError(
                        "No module named 'yamaa._native'", name=fullname
                    )
                return None


        sys.meta_path.insert(0, _NativeBlocker())
        sys.modules.pop("yamaa._native", None)

        spec = importlib.util.spec_from_file_location(
            "test_style_under_test", {path!r}
        )
        module = importlib.util.module_from_spec(spec)
        try:
            spec.loader.exec_module(module)
        except BaseException as exc:  # pytest's Skipped is a BaseException
            if type(exc).__name__ == "Skipped":
                print(f"SKIPPED: {{exc}}")
                raise SystemExit(0) from exc
            raise
        print("style tests were not skipped")
        raise SystemExit(1)
        """
    ).format(path=str(STYLE_TEST))
    result = subprocess.run(
        [sys.executable, "-c", script],
        check=False,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "SKIPPED" in result.stdout
