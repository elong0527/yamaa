"""Boundary controls for the 1,024-column ceilings (#2185, #2288 item 2).

The shared compiler admits 1,024 declared columns, output columns and
typed source fields, and refuses 1,025. These controls exercise the
installed public host API, so they need the compiled native extension.
"""

import json

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
    path = "input.csv"
    if source_format == "parquet":
        path = "input.parquet"
        pq.write_table(
            pa.table({name: ["S-001" if name == "ID" else "x"] for name in header}),
            tmp_path / "input.parquet",
        )
    # A dataset is written as its bare path unless it also types fields.
    source = path
    if typed_fields:
        source = {"path": path, "types": {name: "str" for name in header}}
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


def project_file(tmp_path):
    """A project that lists spec.yaml and links an empty Python environment."""
    (tmp_path / "environment.yaml").write_text("schema_version: '1.0'\n")
    project = tmp_path / "project.yaml"
    project.write_text(
        "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n"
        "adam: {specs: [spec.yaml]}\n"
    )
    return project


def operations(spec, project):
    """Check and run one specification, alone or as its project names it."""
    specification = (
        yamaa.spec(spec) if project is None else yamaa.project(project).spec("TEST")
    )
    return specification.check(), specification.run()


def refusals(spec, project):
    """The one finding of checking and of running a specification without a name.

    A project selects only a specification it can read a `domain` from, so
    through a project the refusal comes from checking and running the project,
    and each finding there also names the listed file as `entry` and `source`.
    """
    if project is None:
        frames = [result.issues for result in operations(spec, None)]
    else:
        listed = yamaa.project(project)
        frames = [listed.check().issues, listed.run().issues]
    findings = []
    for frame in frames:
        assert frame.height == 1
        finding = frame.row(0, named=True)
        finding["context"] = json.loads(finding["context"])
        if project is not None:
            identity = spec.resolve().as_posix()
            assert finding["context"].pop("entry") == identity
            assert finding["context"].pop("source") == identity
        findings.append(finding)
    return findings


def test_declared_and_output_columns_admit_1024_and_refuse_1025(tmp_path):
    spec = write_case(tmp_path, columns=1024)
    assert yamaa.spec(spec).check().issues.height == 0
    built = yamaa.spec(spec).run()
    assert built.issues.height == 0
    assert built.output.width == 1024
    assert built.output.row(0) == ("S-001", *["x"] * 1023)

    spec = write_case(tmp_path, columns=1025)
    assert compiler_limit_rows(yamaa.spec(spec).check())


def test_typed_source_fields_admit_1024_and_refuse_1025(tmp_path):
    spec = write_case(tmp_path, columns=1, typed_fields=1024)
    assert yamaa.spec(spec).check().issues.height == 0
    built = yamaa.spec(spec).run()
    assert built.issues.height == 0
    assert built.output.to_dict(as_series=False) == {"ID": ["S-001"]}

    spec = write_case(tmp_path, columns=1, typed_fields=1025)
    assert compiler_limit_rows(yamaa.spec(spec).check())


@pytest.mark.parametrize("source_format", ["csv", "parquet"])
def test_source_width_admits_1024_and_refuses_1025(tmp_path, source_format):
    for width in (1024, 1025):
        spec = write_case(
            tmp_path, columns=1, typed_fields=width, source_format=source_format
        )
        document = yaml.safe_load(spec.read_text())
        # Without its types, the dataset is written as its bare path.
        document["input"]["SRC"] = document["input"]["SRC"]["path"]
        spec.write_text(yaml.safe_dump(document))
        built = yamaa.spec(spec).run()
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
    assert yamaa.spec(spec).check().issues.height == 0
    # Callers build and save the producer before the consumer reads its artifact.
    yamaa.spec(producer).run().save()
    built = yamaa.spec(spec).run()
    assert built.issues.height == 0
    assert built.output.to_dict(as_series=False) == {"ID": ["S-001"]}


@pytest.mark.parametrize("project", [False, True])
@pytest.mark.parametrize(
    ("resource", "limit", "actual", "path"),
    [
        ("columns", 1024, 1025, "columns"),
        ("inputs", 16, 17, "input"),
        ("source_fields", 1024, 1025, "input.SRC.types"),
        ("projected_columns", 1024, 1025, "output.columns"),
    ],
)
def test_compiler_refusal_details_on_check_and_run(
    tmp_path, project, resource, limit, actual, path
):
    spec = write_case(
        tmp_path,
        columns=actual if resource == "columns" else 1,
        typed_fields=actual if resource == "source_fields" else 0,
    )
    document = yaml.safe_load(spec.read_text())
    if resource == "inputs":
        document["input"] = {f"S{index}": "input.csv" for index in range(actual)}
        # Every specification is pruned after composition (LAYER-11 and LAYER-12):
        # keep all 17 inputs live through a single row-phase default so the
        # inputs capacity is still exercised instead of being pruned away.
        document["columns"][0]["derivation"] = {
            "first_available": {"sources": [f"S{index}.ID" for index in range(actual)]}
        }
    if resource == "projected_columns":
        document["output"]["columns"] = ["ID"] * actual
    spec.write_text(yaml.safe_dump(document))
    for result in operations(spec, project_file(tmp_path) if project else None):
        assert result.issues.height == 1
        issue = result.issues.row(0, named=True)
        assert issue["phase"] == "engine"
        assert issue["condition"] == "engine_rejected"
        assert issue["rule"] is None
        assert issue["spec_paths"] == [path]
        assert json.loads(issue["context"]) == {
            "code": "compiler_limit",
            "stage": "prepare",
            "resource": resource,
            "limit": limit,
            "actual": actual,
        }


@pytest.mark.parametrize("project", [False, True])
def test_inheritance_depth_admits_original_chain_and_parent_occurrences_keep_fixed_bound(
    tmp_path, project
):
    spec = write_case(tmp_path, columns=1)
    (tmp_path / "base.yaml").write_text(spec.read_text())
    for index in range(65):
        parent = f"layer-{index + 1}.yaml" if index < 64 else "base.yaml"
        (tmp_path / f"layer-{index}.yaml").write_text(
            f"schema_version: '1.0'\nparent: {parent}\n"
        )
    spec.write_text("schema_version: '1.0'\nparent: layer-0.yaml\n")
    project = project_file(tmp_path) if project else None
    checked, built = operations(spec, project)
    assert checked.issues.height == 0
    assert built.issues.height == 0
    assert built.output.to_dict(as_series=False) == {"ID": ["S-001"]}
    built.save()
    assert (tmp_path / "out.csv").read_bytes() == b"ID\nS-001\n"

    # Each layer names one parent, so only a chain of 8,193 distinct layers
    # reaches the occurrence bound; without the bound it would reach the base.
    for index in range(1, 8193):
        (tmp_path / f"chain-{index}.yaml").write_text(
            f"schema_version: '1.0'\nparent: chain-{index + 1}.yaml\n"
        )
    (tmp_path / "chain-8193.yaml").write_text((tmp_path / "base.yaml").read_text())
    spec.write_text("schema_version: '1.0'\nparent: chain-1.yaml\n")
    for issue in refusals(spec, project):
        assert issue["phase"] == "engine"
        assert issue["condition"] == "engine_rejected"
        assert issue["rule"] is None
        assert issue["spec_paths"] == []
        assert issue["context"] == {
            "code": "resource_limit",
            "stage": "prepare",
            "phase": "inheritance",
            "resource": "parent_occurrences",
            "limit": 8192,
            "actual": 8193,
        }
    assert (tmp_path / "out.csv").read_bytes() == b"ID\nS-001\n"


@pytest.mark.parametrize("project", [False, True])
def test_wide_yaml_integer_reaches_its_complete_language_finding(tmp_path, project):
    spec = write_case(tmp_path, columns=1)
    digits = "9" * 4097
    project = project_file(tmp_path) if project else None
    for text, actual in (
        ("value: " + digits + "\n", None),
        ("schema_version: " + digits + "\n", int(digits)),
    ):
        spec.write_text(text)
        for issue in refusals(spec, project):
            assert issue["phase"] == "validation"
            assert issue["condition"] == "schema_version_mismatch"
            assert issue["rule"] == "SCHEMA-2"
            assert issue["spec_paths"] == ["schema_version"]
            assert issue["context"] == {"expected": "1.0", "actual": actual}
    assert not (tmp_path / "out.csv").exists()
