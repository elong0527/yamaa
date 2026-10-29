import subprocess
import sys
import textwrap
from importlib.metadata import distribution, version
from pathlib import Path

import yamaa

REMOVED = (
    "domain",
    "check",
    "define",
    "resolve_specification",
    "write_odm_parquet",
    "ResolvedSpecification",
    "Diagnostic",
    "SourceSpan",
    "DomainError",
    "DefineError",
    "ResolvedSpecificationError",
    "resources",
    "yamaa_domain",
    "DomainRun",
    "DomainRunError",
    "generate_study_document",
)


def test_installed_package_exports_exactly_the_public_api() -> None:
    assert yamaa.__version__ == version("yamaa")
    assert yamaa.__all__ == [
        "Error",
        "StateError",
        "Table",
        "__version__",
        "conditions",
        "info",
        "lint",
        "project",
        "read_odm",
        "schema",
        "selftest",
        "spec",
    ]
    for name in (
        "spec",
        "project",
        "read_odm",
        "lint",
        "schema",
        "conditions",
        "info",
        "selftest",
    ):
        assert callable(getattr(yamaa, name))
    assert yamaa.Table is yamaa._native.Table
    assert issubclass(yamaa.Error, Exception)
    for name in REMOVED:
        assert name not in dir(yamaa), name
        assert not hasattr(yamaa, name), name


def test_vendored_package_without_distribution_metadata_uses_core_version() -> None:
    # A vendored copy has the compiled extension but no dist-info, so the
    # metadata lookup raises PackageNotFoundError; importing the package
    # must fall back to the core version, in its Python spelling, instead
    # of raising.
    code = textwrap.dedent(
        """
        import importlib.metadata as metadata

        real_version = metadata.version
        installed = real_version("yamaa")


        def missing(name, *args, **kwargs):
            if name == "yamaa":
                raise metadata.PackageNotFoundError(name)
            return real_version(name, *args, **kwargs)


        metadata.version = missing

        import yamaa

        assert yamaa.__version__ == installed, (yamaa.__version__, installed)
        """
    )
    subprocess.run([sys.executable, "-c", code], check=True)


def test_one_installed_distribution_owns_the_facade_and_native_extension() -> None:
    from yamaa import _native

    package = distribution("yamaa")
    owned = {package.locate_file(path).resolve() for path in package.files}
    assert Path(yamaa.__file__).resolve() in owned
    assert Path(_native.__file__).resolve() in owned


def test_installed_distribution_contains_only_the_retained_modules() -> None:
    package = distribution("yamaa")
    assert package.files
    entries = {
        entry.parts[1]
        for entry in package.files
        if len(entry.parts) > 1 and entry.parts[0] == "yamaa"
    }
    extensions = {
        name
        for name in entries
        if name.startswith("_native.") and name != "_native.pyi"
    }
    assert len(extensions) == 1, extensions
    assert Path(next(iter(extensions))).suffix in {".so", ".pyd"}
    assert entries - extensions == {
        "__init__.py",
        "_locked_functions.py",
        "_native.pyi",
        "_support.py",
        "py.typed",
    }
