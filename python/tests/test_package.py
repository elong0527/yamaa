from importlib.metadata import version

import yamaa
from yamaa.odm import iter_odm_records, read_odm, write_odm_parquet
from yamaa.specification import load_specification


def test_installed_package_exports_public_helpers() -> None:
    assert yamaa.__version__ == version("yamaa")
    assert callable(iter_odm_records)
    assert callable(read_odm)
    assert callable(write_odm_parquet)
    assert callable(load_specification)
