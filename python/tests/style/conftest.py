"""Every text a style test lints with the complete finding set is also linted
by the Rust lint, which must agree with the Python tool on it (equality.py)."""

from pathlib import Path

try:
    import yamaa.style as style
    from yamaa.style._checks import FINDINGS
except ImportError:  # test_style.py skips without the native extension
    style = None

if style is not None:
    from equality import assert_equal

    original_check, original_fix = style._check_file, style._fix_file

    def complete(select):
        return select is None or set(select) == set(FINDINGS)

    def check_file(path, *, select=None, orders=None):
        if complete(select):
            assert_equal(Path(path).read_bytes().decode("utf-8"))
        return original_check(path, select=select, orders=orders)

    def fix_file(path, *, select=None, orders=None, write=True):
        if complete(select):
            assert_equal(Path(path).read_bytes().decode("utf-8"))
        return original_fix(path, select=select, orders=orders, write=write)

    style._check_file = check_file
    style._fix_file = fix_file
