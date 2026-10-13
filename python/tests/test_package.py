from importlib.metadata import distribution, version
from pathlib import Path

import yamaa


def test_installed_package_exports_public_helpers() -> None:
    assert yamaa.__version__ == version("yamaa")
    assert set(yamaa.__all__) == {
        "DefineError",
        "DomainError",
        "__version__",
        "check",
        "define",
        "domain",
        "read_odm",
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
