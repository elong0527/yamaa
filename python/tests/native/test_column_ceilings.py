"""Boundary controls for the 1,024-column ceilings (#2185, #2288 item 2).

The shared compiler admits 1,024 declared columns, output columns and
typed source fields, and refuses 1,025. These controls exercise the
installed public host API, so they need the compiled native extension.
"""

import pyarrow as pa
import pyarrow.parquet as pq
import pytest
import yaml

pytest.importorskip(
    "yamaa._native", reason="the compiled native extension is not installed"
)

import yamaa


def write_case(tmp_path, *, columns, typed_fields=0, source_format="csv"):
    names = ["ID"] + [f"C{index:04d}" for index in range(1, columns)]
    field_names = [f"F{index:04d}" for index in range(1, typed_fields)]
    header = ["ID", *field_names]
    (tmp_path / "input.csv").write_text(
        ",".join(header) + "\n" + ",".join(["S-001", *["x"] * len(field_names)]) + "\n"
    )
    source = {"path": "input.csv"}
    if typed_fields:
        source["types"] = {name: "str" for name in header}
    if source_format == "parquet":
        source["path"] = "input.parquet"
        pq.write_table(
            pa.table({name: ["S-001" if name == "ID" else "x"] for name in header}),
            tmp_path / "input.parquet",
        )
    document = {
        "schema_version": "1.0",
        "domain": "TEST",
        "keys": ["ID"],
        "input": {"SRC": source},
        "columns": [
            {"name": "ID", "type": "str", "label": "ID", "derivation": "SRC.ID"},
            *[
                {
                    "name": name,
                    "type": "str",
                    "label": name,
                    "derivation": {"literal": "x"},
                }
                for name in names[1:]
            ],
        ],
        "output": {"path": "out.csv", "columns": names},
    }
    spec = tmp_path / "spec.yaml"
    spec.write_text(yaml.safe_dump(document))
    return spec


def compiler_limit_rows(result):
    issues = result.issues
    if issues.height == 0:
        return []
    return [
        row
        for row in issues.iter_rows(named=True)
        if row["condition"] == "engine_rejected" and "compiler_limit" in row["context"]
    ]


def test_declared_and_output_columns_admit_1024_and_refuse_1025(tmp_path):
    spec = write_case(tmp_path, columns=1024)
    assert yamaa.check(spec).issues.height == 0
    built = yamaa.domain(spec)
    assert built.issues.height == 0
    assert built.output.width == 1024
    assert built.output.row(0) == ("S-001", *["x"] * 1023)

    spec = write_case(tmp_path, columns=1025)
    assert compiler_limit_rows(yamaa.check(spec))


def test_typed_source_fields_admit_1024_and_refuse_1025(tmp_path):
    spec = write_case(tmp_path, columns=1, typed_fields=1024)
    assert yamaa.check(spec).issues.height == 0
    built = yamaa.domain(spec)
    assert built.issues.height == 0
    assert built.output.to_dict(as_series=False) == {"ID": ["S-001"]}

    spec = write_case(tmp_path, columns=1, typed_fields=1025)
    assert compiler_limit_rows(yamaa.check(spec))


@pytest.mark.parametrize("source_format", ["csv", "parquet"])
def test_source_width_admits_1024_and_refuses_1025(tmp_path, source_format):
    for width in (1024, 1025):
        spec = write_case(
            tmp_path, columns=1, typed_fields=width, source_format=source_format
        )
        document = yaml.safe_load(spec.read_text())
        del document["input"]["SRC"]["types"]
        spec.write_text(yaml.safe_dump(document))
        built = yamaa.domain(spec)
        if width == 1024:
            assert built.issues.height == 0
            assert built.output.to_dict(as_series=False) == {"ID": ["S-001"]}
        else:
            assert built.output is None
            assert "engine_rejected" in built.issues["condition"].to_list()


def test_wide_producer_contract_and_decoder_build(tmp_path):
    producer = write_case(tmp_path, columns=1024)
    consumer = yaml.safe_load(producer.read_text())
    consumer["input"]["SRC"] = {
        "path": "out.csv",
        "schema": "spec.yaml",
    }
    consumer["columns"] = consumer["columns"][:1]
    consumer["output"] = {"path": "consumer.csv", "columns": ["ID"]}
    spec = tmp_path / "consumer.yaml"
    spec.write_text(yaml.safe_dump(consumer))
    assert yamaa.check(spec).issues.height == 0
    built = yamaa.domain(spec)
    assert built.issues.height == 0
    assert built.output.to_dict(as_series=False) == {"ID": ["S-001"]}
