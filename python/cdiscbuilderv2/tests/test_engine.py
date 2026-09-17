import pytest
import polars as pl
from pathlib import Path
import yaml
from cdiscbuilderv2.engine import CDISCEngine


@pytest.fixture
def dummy_data_dir(tmp_path):
    df1 = pl.DataFrame({
        "SUBJID": ["1", "2", "3", "3"],
        "AGE": [25, 30, 45, 45],
        "SEX": ["Male", "Female", "Unknown", "Male"],
        "WEIGHT": [70.5, 65.0, 80.0, 82.0],
        "DATE": ["2020-01-01", "2020-02-01", "2020-03-01", "2020-04-01"]
    })
    df1_path = tmp_path / "raw_data.parquet"
    df1.write_parquet(df1_path)
    return str(tmp_path)


def test_engine_basic_derivation(dummy_data_dir, tmp_path):
    spec = {
        "domain": "DM",
        "input": {
            "raw": f"{dummy_data_dir}/raw_data.parquet"
        },
        "base": "raw",
        "columns": [
            {
                "name": "DOMAIN",
                "derivation": {"literal": "DM"}
            },
            {
                "name": "USUBJID",
                "derivation": {"source": "SUBJID"}
            },
            {
                "name": "AGE_PLUS_10",
                "derivation": {"add": {"source": "AGE", "addend": 10.0}}
            },
            {
                "name": "SEX_MAPPED",
                "derivation": {
                    "mapping": {
                        "source": "SEX",
                        "dict": {"Male": "M", "Female": "F"},
                        "unmapped": "U"
                    }
                }
            },
            {
                "name": "SEQ",
                "derivation": {
                    "row_number": {
                        "group_by": ["SUBJID"]
                    }
                }
            }
        ],
        "rows": [
            {
                "id": "1",
                "filter": "AGE = 25",
                "derivations": {
                    "AGE_PLUS_10": {"literal": 999.0}
                }
            }
        ]
    }

    spec_path = tmp_path / "dm.yaml"
    with open(spec_path, "w") as f:
        yaml.dump(spec, f)

    engine = CDISCEngine(str(spec_path))
    result_df = engine.build()

    assert "DOMAIN" in result_df.columns
    assert result_df["DOMAIN"][0] == "DM"
    assert result_df["USUBJID"][0] == "1"
    assert result_df["AGE_PLUS_10"][0] == 999.0  # From row filter override
    assert result_df["SEX_MAPPED"][0] == "M"


def test_engine_filtered_source(dummy_data_dir, tmp_path):
    # Tests post-#517 filtered source features:
    # 1. source: {variable: ..., filter: ...}
    # 2. mapping: {source: {variable: ..., filter: ...}, ...}
    spec = {
        "domain": "DM",
        "input": {
            "raw": f"{dummy_data_dir}/raw_data.parquet"
        },
        "base": "raw",
        "columns": [
            {
                "name": "SUBJID",
                "derivation": {"source": "SUBJID"}
            },
            {
                "name": "FILTERED_SEX",
                "derivation": {
                    "source": {
                        "variable": "SEX",
                        "filter": "AGE > 25"
                    }
                }
            },
            {
                "name": "FILTERED_MAPPED_SEX",
                "derivation": {
                    "mapping": {
                        "source": {
                            "variable": "SEX",
                            "filter": "AGE > 25"
                        },
                        "dict": {"Female": "F", "Male": "M"}
                    }
                }
            }
        ]
    }

    spec_path = tmp_path / "dm_filtered.yaml"
    with open(spec_path, "w") as f:
        yaml.dump(spec, f)

    engine = CDISCEngine(str(spec_path))
    result_df = engine.build()

    # Row 0 has AGE = 25 -> filter "AGE > 25" is false, so FILTERED_SEX should be null
    assert result_df["FILTERED_SEX"][0] is None
    # Row 1 has AGE = 30 -> filter "AGE > 25" is true, so FILTERED_SEX should be "Female"
    assert result_df["FILTERED_SEX"][1] == "Female"
    assert result_df["FILTERED_MAPPED_SEX"][1] == "F"


def test_engine_save(dummy_data_dir, tmp_path):
    spec = {
        "domain": "AE",
        "input": {
            "raw": f"{dummy_data_dir}/raw_data.parquet"
        },
        "base": "raw",
        "columns": [
            {"name": "TEST", "derivation": {"literal": 1}}
        ]
    }
    spec_path = tmp_path / "ae.yaml"
    with open(spec_path, "w") as f:
        yaml.dump(spec, f)

    engine = CDISCEngine(str(spec_path))
    engine.build()

    output_dir = tmp_path / "output"
    saved = engine.save(str(output_dir), formats=["parquet", "csv", "xpt"])

    assert (output_dir / "ae.parquet").exists()
    assert (output_dir / "ae.csv").exists()
    assert (output_dir / "ae.xpt").exists()
