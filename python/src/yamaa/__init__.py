"""Run yamaa clinical data specifications through the shared Rust engine.

`spec()` and `project()` read nothing; each method runs one engine operation
and returns a fixed result, and only a result's `save()` writes files.
"""

from importlib.metadata import PackageNotFoundError as _PackageNotFoundError
from importlib.metadata import version as _distribution_version
from os import fspath as _fspath

import polars as _pl

from yamaa import _native

__all__ = ["Error", "__version__", "project", "read_odm", "spec"]


class Error(Exception):
    """A yamaa operation could not complete; `issues` says why."""

    def __init__(self, message: str, issues: _pl.DataFrame):
        super().__init__(message)
        self.issues = issues


def _call(function, *args):
    try:
        return function(*args)
    except _native._Error as error:
        # _Error is a native exception class; the binding sets `issues` on
        # it, so it is read as a dynamic attribute.
        raise Error(str(error), _pl.DataFrame(getattr(error, "issues"))) from None


def _frame(table):
    return None if table is None else _pl.DataFrame(table)


class Check:
    """Static findings; `issues` is empty when nothing is wrong."""

    def __init__(self, table):
        self.issues = _pl.DataFrame(table)


class Run:
    """One specification run; nothing is written until `save()`."""

    def __init__(self, native):
        self._native = native

    @property
    def output(self) -> _pl.DataFrame | None:
        """The output dataset, or None when the run failed."""
        return _frame(self._native.output())

    @property
    def issues(self) -> _pl.DataFrame:
        """What is wrong, one row per finding."""
        return _pl.DataFrame(self._native.issues())

    @property
    def report(self) -> _pl.DataFrame | None:
        """What the run read, counted and checked."""
        return _frame(self._native.report())

    def save(self) -> None:
        """Write the declared report file and the output dataset."""
        _call(self._native.save)


class MemberRun:
    """One specification's run inside a project run; save the project run."""

    def __init__(self, native, name: str):
        self._native = native
        self._name = name

    @property
    def output(self) -> _pl.DataFrame | None:
        return _frame(_call(self._native.output, self._name))

    @property
    def issues(self) -> _pl.DataFrame:
        return _pl.DataFrame(_call(self._native.member_issues, self._name))

    @property
    def report(self) -> _pl.DataFrame | None:
        return _frame(_call(self._native.member_report, self._name))


class ProjectRun:
    """Every specification a project run built, by `domain` name."""

    def __init__(self, native):
        self._native = native

    @property
    def names(self) -> list[str]:
        """The specifications in the run scope, in run order."""
        return self._native.names()

    def __getitem__(self, name: str) -> MemberRun:
        if name not in self._native.names():
            _call(self._native.member_issues, name)  # raises the engine's refusal
        return MemberRun(self._native, name)

    def __contains__(self, name: object) -> bool:
        return name in self._native.names()

    def __iter__(self):
        """The names, as `names` lists them."""
        return iter(self._native.names())

    @property
    def issues(self) -> _pl.DataFrame:
        return _pl.DataFrame(self._native.issues())

    @property
    def report(self) -> _pl.DataFrame:
        return _pl.DataFrame(self._native.report())

    def save(self) -> None:
        _call(self._native.save)


class Submission:
    """define.xml and each selected section's Dataset-JSON or SAS Transport files."""

    def __init__(self, native):
        self._native = native

    @property
    def issues(self) -> _pl.DataFrame:
        return _pl.DataFrame(self._native.issues())

    def save(self) -> None:
        _call(self._native.save)


class Spec:
    """One specification, read only when a method runs."""

    def __init__(self, path, project=None):
        self._path = _fspath(path)
        self._project = project

    def check(self) -> Check:
        """Report problems without reading study data or running project code."""
        return Check(_call(_native._spec_check, self._path, self._project))

    def resolve(self) -> dict:
        """The specification after its layers are composed."""
        return _call(_native._spec_resolve, self._path, self._project)

    def run(self) -> Run:
        """Build the dataset in memory."""
        return Run(_call(_native._spec_run, self._path, self._project))


class Project:
    """A study's project file, read only when a method runs."""

    def __init__(self, path):
        self._path = _fspath(path)

    def spec(self, name: str) -> Spec:
        """The listed specification whose `domain` is `name`."""
        return Spec(_call(_native._project_spec, self._path, name), self._path)

    def check(self) -> Check:
        return Check(_call(_native._project_check, self._path))

    def run(self, section: str | None = None) -> ProjectRun:
        """Build every specification of one section, or of all sections."""
        return ProjectRun(_call(_native._project_run, self._path, section))

    def submission(
        self, section: str | None = None, *, created: str | None = None
    ) -> Submission:
        return Submission(
            _call(_native._project_submission, self._path, section, created)
        )


def spec(path) -> Spec:
    """One specification; nothing is read until a method runs."""
    return Spec(path)


def project(path) -> Project:
    """A project file; nothing is read until a method runs."""
    return Project(path)


def read_odm(
    source,
    *,
    archive_member: str | None = None,
    max_archive_members: int = 10_000,
    max_expanded_bytes: int = 256 * 1024 * 1024,
    max_xml_depth: int = 64,
) -> _pl.DataFrame:
    """Read the clinical items of an ODM XML file, or of one member of a TAR."""
    return _pl.DataFrame(
        _call(
            _native._read_odm,
            _fspath(source),
            archive_member,
            max_archive_members,
            max_expanded_bytes,
            max_xml_depth,
        )
    )


try:
    __version__ = _distribution_version("yamaa")
except _PackageNotFoundError:
    # A vendored copy carries no distribution metadata. The compiled core
    # version is the same release: the shared-release check keeps the
    # Python, R and Rust versions equal.
    from yamaa._native import _CORE_VERSION as __version__
