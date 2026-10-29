"""Handwritten support for the public module, which is generated.

`yamaa/__init__.py` is generated from rust/interface/bindings.yaml by
tools/packaging/generate_bindings.py: its classes and functions delegate each
operation to `yamaa._native`, and every table is the native `yamaa.Table`.
What it does not write lives here: path conversion, the pickling of lazy
objects and the package version. Nothing here imports a package outside the
standard library.
"""

from importlib.metadata import PackageNotFoundError as _PackageNotFoundError
from importlib.metadata import version as _distribution_version
from os import PathLike
from os import fspath as _os_fspath

from yamaa import _native

__all__ = [
    "__version__",
    "_fspath",
    "_fspaths",
    "_optional_fspath",
    "_Lazy",
]


def _fspath(path) -> str:
    """A host argument refusal has the native refusal class and an issues table."""
    try:
        result = _os_fspath(path)
        if not isinstance(result, str):
            raise TypeError("paths must be text, not bytes")
        return result
    except TypeError as error:
        raise _native.Error(str(error), _native._empty_issues(), []) from None


def _text(value, what):
    if not isinstance(value, str):
        raise _native.Error(f"{what} must be text", _native._empty_issues(), [])
    return value


def _optional_text(value, what):
    return None if value is None else _text(value, what)


def _flag(value, what):
    if not isinstance(value, bool):
        raise _native.Error(f"{what} must be a Boolean", _native._empty_issues(), [])
    return value


def _fspaths(paths) -> list[str]:
    """One path or a list of paths, each as a string."""
    if isinstance(paths, (str, PathLike)):
        return [_fspath(paths)]
    try:
        return [_fspath(path) for path in paths]
    except TypeError as error:
        raise _native.Error(str(error), _native._empty_issues(), []) from None


def _optional_fspath(path) -> str | None:
    """A path as a string, or None for no path."""
    return None if path is None else _fspath(path)


class _Lazy:
    """A lazy object's native plans holder stays in its process: a copy, a
    pickle or another process starts with an empty one and compiles again."""

    def __getstate__(self):
        state = self.__dict__.copy()
        del state["_plans"]
        return state

    def __setstate__(self, state):
        self.__dict__.update(state)
        self._plans = _native._plans()


try:
    __version__ = _distribution_version("yamaa")
except _PackageNotFoundError:
    # A vendored copy carries no distribution metadata. The compiled core
    # version names the same release in its Cargo spelling, which differs
    # only for a release candidate: Cargo 1.0.0-rc.1 is Python 1.0.0rc1.
    __version__ = _native._CORE_VERSION.replace("-rc.", "rc")
