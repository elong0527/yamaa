import subprocess
import sys
import textwrap
from importlib.metadata import distribution, version
from pathlib import Path

import yamaa


def test_installed_package_exports_public_helpers() -> None:
    assert yamaa.__version__ == version("yamaa")
    assert set(yamaa.__all__) == {
        "DefineError",
        "Diagnostic",
        "DomainError",
        "ResolvedSpecification",
        "ResolvedSpecificationError",
        "SourceSpan",
        "__version__",
        "check",
        "define",
        "domain",
        "read_odm",
        "resolve_specification",
        "write_odm_parquet",
    }
    for name in ("domain", "check", "define", "read_odm", "write_odm_parquet"):
        assert callable(getattr(yamaa, name))
    for name in (
        "yamaa_domain",
        "DomainRun",
        "DomainRunError",
        "generate_study_document",
    ):
        assert name not in dir(yamaa)
        assert not hasattr(yamaa, name)


def test_vendored_package_without_distribution_metadata_uses_core_version() -> None:
    # A vendored copy has the compiled extension but no dist-info, so the
    # metadata lookup raises PackageNotFoundError; importing the package
    # must fall back to the core version instead of raising.
    code = textwrap.dedent(
        """
        import importlib.metadata as metadata

        real_version = metadata.version


        def missing(name, *args, **kwargs):
            if name == "yamaa":
                raise metadata.PackageNotFoundError(name)
            return real_version(name, *args, **kwargs)


        metadata.version = missing

        import yamaa
        from yamaa import _native

        assert yamaa.__version__ == _native._CORE_VERSION, yamaa.__version__
        """
    )
    subprocess.run([sys.executable, "-c", code], check=True)


def test_define_passes_core_version_without_distribution_metadata(
    monkeypatch,
) -> None:
    # define() must hand the native generator the compiled core version,
    # as the R host does, never the installed distribution metadata.
    from yamaa import _define, _native

    captured = {}

    class _Native:
        def issues(self):
            return []

    def fake_define_file(environment, standard, created, passed_version):
        captured.update(
            environment=environment,
            standard=standard,
            created=created,
            version=passed_version,
        )
        return _Native()

    def forbidden_version(name):
        raise AssertionError("define() consulted distribution metadata")

    monkeypatch.setattr(_define, "_define_file", fake_define_file)
    monkeypatch.setattr(_define, "version", forbidden_version, raising=False)
    monkeypatch.setattr("importlib.metadata.version", forbidden_version, raising=False)
    result = _define.define("environment", "sdtm")
    assert captured == {
        "environment": "environment",
        "standard": "sdtm",
        "created": None,
        "version": _native._CORE_VERSION,
    }
    assert result.issues.height == 0


def test_one_installed_distribution_owns_the_facade_and_native_extension() -> None:
    from yamaa import _native

    package = distribution("yamaa")
    owned = {package.locate_file(path).resolve() for path in package.files}
    assert Path(yamaa.__file__).resolve() in owned
    assert Path(_native.__file__).resolve() in owned
    assert callable(yamaa.domain)
    assert callable(yamaa.check)
    assert issubclass(yamaa.DomainError, Exception)


def test_installed_distribution_contains_only_retained_python_packages() -> None:
    removed = {
        "_reference_domain.py",
        "application.py",
        "regex.py",
        "adapters",
        "expressions",
        "io",
        "models",
        "odm",
        "planning",
        "runtime",
        "schema",
        "specification",
        "submission",
        "verification",
    }
    package = distribution("yamaa")
    assert package.files
    for entry in package.files:
        parts = entry.parts
        if len(parts) > 1 and parts[0] == "yamaa":
            assert parts[1] not in removed, entry
