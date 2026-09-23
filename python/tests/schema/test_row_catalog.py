from pathlib import Path

import pytest

from yamaa.schema.row_catalog import expand_row_catalogs
from yamaa.specification import SpecificationError


def _document() -> dict[str, object]:
    return {
        "rows": [
            {
                "id": "vs",
                "catalog": {
                    "path": "tests.csv",
                    "id_column": "ID",
                    "types": {"ORDER": "int"},
                    "unique_columns": ["CODE", "ORDER"],
                },
                "group_by": ["ODM.SubjectKey"],
                "derivations": {
                    "VSTESTCD": {"literal": "${CODE}"},
                    "TESTORD": {"literal": "${ORDER}"},
                    "VSORRES": {
                        "aggregate": {
                            "filter": "ODM.ItemOID = ${ITEM}",
                            "expr": "ONLY(ODM.Value)",
                        }
                    },
                },
            }
        ]
    }


def test_catalog_expands_in_record_order_with_typed_values(tmp_path: Path) -> None:
    (tmp_path / "tests.csv").write_text(
        "ID,CODE,ORDER,ITEM\nsysbp,SYSBP,1,IT.VS.SYSBP\ndiabp,DIABP,2,IT.VS.DIABP\n"
    )
    document = _document()
    expanded = expand_row_catalogs(document, tmp_path / "spec.yaml")
    rows = expanded["rows"]
    assert [row["id"] for row in rows] == ["vs_sysbp", "vs_diabp"]
    assert rows[0]["derivations"]["TESTORD"] == {"literal": 1}
    assert rows[1]["derivations"]["VSORRES"]["aggregate"]["filter"] == (
        "ODM.ItemOID = 'IT.VS.DIABP'"
    )
    assert document["rows"][0]["catalog"]["path"] == "tests.csv"


@pytest.mark.parametrize(
    ("csv_text", "condition"),
    [
        (
            "ID,CODE,ORDER,ITEM\nsysbp,SYSBP,1,IT.VS.SYSBP\nsysbp,DIABP,2,IT.VS.DIABP\n",
            "invalid_row_catalog",
        ),
        (
            "ID,CODE,ORDER,ITEM\nsysbp,SYSBP,1,IT.VS.SYSBP\ndiabp,SYSBP,2,IT.VS.DIABP\n",
            "invalid_row_catalog",
        ),
        ("ID,CODE,ORDER,ITEM\nsysbp,SYSBP,,IT.VS.SYSBP\n", "invalid_row_catalog"),
        ("ID,CODE,ORDER,ITEM\nsysbp,SYSBP,no,IT.VS.SYSBP\n", "invalid_row_catalog"),
    ],
)
def test_catalog_rejects_invalid_records(
    tmp_path: Path, csv_text: str, condition: str
) -> None:
    (tmp_path / "tests.csv").write_text(csv_text)
    with pytest.raises(SpecificationError) as error:
        expand_row_catalogs(_document(), tmp_path / "spec.yaml")
    assert error.value.diagnostics[0].condition == condition
    assert error.value.diagnostics[0].spec_paths == ("rows[0].catalog",)


def test_catalog_rejects_unknown_placeholder(tmp_path: Path) -> None:
    (tmp_path / "tests.csv").write_text(
        "ID,CODE,ORDER,ITEM\nsysbp,SYSBP,1,IT.VS.SYSBP\n"
    )
    document = _document()
    document["rows"][0]["derivations"]["VSTESTCD"] = {"literal": "${UNKNOWN}"}
    with pytest.raises(SpecificationError) as error:
        expand_row_catalogs(document, tmp_path / "spec.yaml")
    assert error.value.diagnostics[0].condition == "unknown_row_catalog_column"


def test_catalog_quotes_predicate_values(tmp_path: Path) -> None:
    (tmp_path / "tests.csv").write_text(
        "ID,CODE,ORDER,ITEM\nsysbp,SYSBP,1,IT.VS.BP'S\n"
    )
    row = expand_row_catalogs(_document(), tmp_path / "spec.yaml")["rows"][0]
    assert row["derivations"]["VSORRES"]["aggregate"]["filter"] == (
        "ODM.ItemOID = 'IT.VS.BP''S'"
    )


def test_catalog_path_stays_under_approved_root(tmp_path: Path) -> None:
    document = _document()
    document["rows"][0]["catalog"]["path"] = "../tests.csv"
    with pytest.raises(SpecificationError) as error:
        expand_row_catalogs(document, tmp_path / "spec.yaml")
    assert error.value.diagnostics[0].spec_paths == ("rows[0].catalog",)
