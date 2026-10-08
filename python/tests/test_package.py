from importlib.metadata import distribution, version
from pathlib import Path

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
    from yamaa import _native

    package = distribution("yamaa")
    owned = {package.locate_file(path).resolve() for path in package.files}
    assert Path(yamaa.__file__).resolve() in owned
    assert Path(_native.__file__).resolve() in owned
    assert callable(yamaa.domain)
    assert callable(yamaa.check)
    assert issubclass(yamaa.DomainError, Exception)
