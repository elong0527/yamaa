from importlib.metadata import distribution, version
from pathlib import Path

import pytest

import yamaa
from yamaa.odm import iter_odm_records, read_odm, write_odm_parquet
from yamaa.specification import load_specification


def test_installed_package_exports_public_helpers() -> None:
    assert yamaa.__version__ == version("yamaa")
    assert callable(yamaa.yamaa_domain)
    assert callable(iter_odm_records)
    assert callable(read_odm)
    assert callable(write_odm_parquet)
    assert callable(load_specification)


def test_one_installed_distribution_owns_the_facade_and_native_extension() -> None:
    # The native extension is optional for the test suite: the rest of the
    # suite runs against the reference implementation, and environments
    # without a compiled yamaa._native (pure-Python dev venvs) should skip
    # this packaging assertion instead of erroring on import.
    _native = pytest.importorskip(
        "yamaa._native", reason="the compiled native extension is not installed"
    )

    package = distribution("yamaa")
    owned = {package.locate_file(path).resolve() for path in package.files}
    assert Path(yamaa.__file__).resolve() in owned
    assert Path(_native.__file__).resolve() in owned
    assert callable(yamaa.domain)
    assert callable(yamaa.check)
    assert issubclass(yamaa.DomainError, Exception)
