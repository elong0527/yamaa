"""Private repository binding witnesses retain their owner and have no IO ports."""

import gc
import json
import shutil
import sys
from pathlib import Path

import pytest
import yaml

from yamaa._native import _prepare_document, _prepare_file_project

ROOT = Path(__file__).resolve().parents[3]


def specification(*, expression="SOURCE.VALUE", kind="int", declared=True):
    source = {"path": "absent.csv"}
    if declared:
        source["types"] = {"VALUE": "int"}
    return {
        "schema_version": "1.0",
        "domain": "META",
        "keys": ["ID"],
        "input": {"SOURCE": source},
        "output": {"path": "out.csv", "columns": ["ID", "VALUE"]},
        "columns": [
            {"name": "ID", "type": "str", "label": "ID", "derivation": "SOURCE.ID"},
            {"name": "VALUE", "type": kind, "label": "VALUE", "derivation": expression},
        ],
    }


def owner(document):
    def refuse(*_arguments):
        raise AssertionError("metadata-only witness entered source IO")

    return _prepare_document(
        "memory/spec.yaml", json.dumps(document).encode(), refuse, refuse, refuse
    )


def fields():
    return [("SOURCE", [("ID", "str"), ("VALUE", "str")])]


def test_metadata_binding_retains_opaque_owner_and_does_not_read_or_execute(tmp_path):
    document = specification(expression={"compute": {"expr": "1 / 0"}}, declared=False)
    document["output"]["path"] = str(tmp_path / "never-published.csv")
    prepared = owner(document)
    del document
    gc.collect()
    assert prepared.check_source_metadata(fields()) == []
    assert not list(tmp_path.iterdir())
    with pytest.raises(TypeError):
        type(prepared)()


def test_native_declaration_types_and_missing_fields_are_diagnostic_truth():
    prepared = owner(specification())
    assert prepared.check_source_metadata(fields()) == []
    [(phase, condition, requirement, paths, context)] = prepared.check_source_metadata(
        [("SOURCE", [("ID", "str"), ("OTHER", "str")])]
    )
    assert (phase, condition, requirement) == (
        "validation",
        "unknown_field",
        "REQ-0532",
    )
    assert paths == ["input.SOURCE.types.VALUE"]
    assert json.loads(context) == {"dataset": "SOURCE", "field": "VALUE"}


@pytest.mark.parametrize(
    "metadata",
    [
        [],
        [("FOREIGN", [("ID", "str"), ("VALUE", "str")])],
        [("SOURCE", [("ID", "str"), ("ID", "str")])],
        [("SOURCE", [("", "str")])],
        [("SOURCE", [("ID", "str"), ("VALUE", "bool")])],
        [("SOURCE", [("ID", "str"), ("VALUE", "int")])],
        [("SOURCE", [("ID", "str"), ("VALUE", None)])],
        [("SOURCE", [("ID", "str"), ("VALUE", "str", "extra")])],
        [("SOURCE", [("ID", "str"), ("VALUE\0", "str")])],
    ],
)
def test_malformed_or_foreign_carriers_never_mutate_the_owner(metadata):
    prepared = owner(specification())
    with pytest.raises((ValueError, TypeError)):
        prepared.check_source_metadata(metadata)
    assert prepared.check_source_metadata(fields()) == []


@pytest.mark.parametrize(
    "metadata",
    [
        [("SOURCE", [(f"F{index}", "str") for index in range(4097)])],
        [("SOURCE", [("x" * 1_048_577, "str")])],
        [(f"S{index}", [("ID", "str")]) for index in range(1025)],
    ],
)
def test_complete_carrier_limits_refuse_before_any_binding_result(metadata):
    prepared = owner(specification())
    with pytest.raises(ValueError):
        prepared.check_source_metadata(metadata)
    assert prepared.check_source_metadata(fields()) == []


def test_project_metadata_binding_never_imports_callbacks_or_recaptures_sources(
    tmp_path,
):
    document = specification(
        expression={"function": {"name": "project_value", "args": {"x": 1.0}}},
        kind="float",
        declared=False,
    )
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(document))
    environment = (
        ROOT / "benchmarks/negative-function-contract/python/environment.yaml"
    ).read_text()
    environment = environment.replace(
        "yamaa_benchmarks.project_value", "yamaa_metadata_callback_sentinel.forbidden"
    )
    (tmp_path / "environment.yaml").write_text(environment)
    shutil.copyfile(ROOT / "python/uv.lock", tmp_path / "uv.lock")
    (tmp_path / "yamaa_metadata_callback_sentinel.py").write_text(
        "raise AssertionError('metadata check activated project code')\n"
    )
    modules = {
        path.name: path.read_bytes() for path in (ROOT / "yaml").glob("schema*.yaml")
    }

    def closure(entry):
        pending, captured = [entry], {}
        while pending:
            name = pending.pop(0)
            if name not in captured:
                captured[name] = modules[name]
                pending.extend(yaml.safe_load(modules[name]).get("includes", []))
        return list(captured.items())

    prepared = _prepare_file_project(
        str(tmp_path),
        str(tmp_path),
        "spec.yaml",
        "environment.yaml",
        [],
        closure("schema.yaml"),
        0,
        closure("schema_environment.yaml"),
        0,
    )
    assert prepared.preparation_status() == "ready"
    captures = prepared.capture_reads()
    assert prepared.check_source_metadata(fields()) == []
    assert prepared.capture_reads() == captures
    assert "yamaa_metadata_callback_sentinel" not in sys.modules
    assert not (tmp_path / "out.csv").exists()


def schema_closure(entry):
    modules = {
        path.name: path.read_bytes() for path in (ROOT / "yaml").glob("schema*.yaml")
    }
    pending, captured = [entry], {}
    while pending:
        name = pending.pop(0)
        if name not in captured:
            captured[name] = modules[name]
            pending.extend(yaml.safe_load(modules[name]).get("includes", []))
    return list(captured.items())


def file_owner(directory, entry="spec.yaml", environment=None):
    return _prepare_file_project(
        str(directory),
        str(directory),
        entry,
        environment,
        [],
        schema_closure("schema.yaml"),
        0,
        schema_closure("schema_environment.yaml"),
        0,
    )


def test_native_original_input_descriptor_and_capture_keep_inherited_anchor(tmp_path):
    (tmp_path / "parent").mkdir()
    (tmp_path / "data").mkdir()
    document = specification()
    document["input"]["SOURCE"]["path"] = "../data/source.csv"
    (tmp_path / "parent/base.yaml").write_text(yaml.safe_dump(document))
    (tmp_path / "spec.yaml").write_text(
        "schema_version: '1.0'\nparents: [parent/base.yaml, './parent/base.yaml']\n"
    )
    payload = b"ID,VALUE\nA,1\n"
    (tmp_path / "data/source.csv").write_bytes(payload)
    prepared = file_owner(tmp_path)
    assert prepared.preparation_status() == "ready", prepared.check_issues()
    reads = prepared.capture_reads()
    identity = (tmp_path / "spec.yaml").as_posix()
    assert prepared.input_metadata() == [
        (
            identity,
            "SOURCE",
            (tmp_path / "parent/base.yaml").as_posix(),
            "../data/source.csv",
            "csv",
        )
    ]
    assert prepared.check_source_metadata([(identity, *fields()[0])]) == []
    with pytest.raises(ValueError, match="retained specification"):
        prepared.check_source_metadata([("foreign/spec.yaml", *fields()[0])])
    for index, maximum in [(1, 1024), (0, 0), (0, 8_388_609)]:
        value, issues = prepared.capture_input_metadata(index, maximum)
        assert value is None and issues
        assert prepared.capture_reads() == reads
    assert prepared.capture_input_metadata(0, 1024) == (payload, [])
    assert prepared.capture_reads() == reads + 1
    assert not (tmp_path / "parent/out.csv").exists()
    (tmp_path / "data/source.csv").write_bytes(b"ID,VALUE\nchanged,2\n")
    value, issues = prepared.capture_input_metadata(0, 1024)
    assert value is None and issues
    assert prepared.capture_reads() == reads + 1


def test_native_capture_keeps_missing_and_symlink_refusals(tmp_path):
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(specification()))
    prepared = file_owner(tmp_path)
    assert prepared.preparation_status() == "ready", prepared.check_issues()
    reads = prepared.capture_reads()
    value, issues = prepared.capture_input_metadata(0, 1024)
    assert value is None
    assert [(row[1], row[2], row[3]) for row in issues] == [
        ("resource_path_missing", "REQ-0785", ["input.SOURCE.path"])
    ]
    assert prepared.capture_reads() == reads

    (tmp_path / "real.csv").write_bytes(b"ID,VALUE\nA,1\n")
    try:
        (tmp_path / "absent.csv").symlink_to(tmp_path / "real.csv")
    except OSError:
        pytest.skip("this host cannot create a symlink")
    value, issues = prepared.capture_input_metadata(0, 1024)
    assert value is None
    assert [(row[1], row[2], row[3]) for row in issues] == [
        ("resource_path_symlink", "REQ-0783", ["input.SOURCE.path"])
    ]
    assert prepared.capture_reads() == reads


def test_inherited_missing_source_never_retries_a_readable_entry_relative_file(
    tmp_path,
):
    (tmp_path / "parent").mkdir()
    (tmp_path / "input").mkdir()
    (tmp_path / "input/data.csv").write_bytes(b"ID,VALUE\nwrong,1\n")
    document = specification()
    document["input"]["SOURCE"]["path"] = "input/data.csv"
    (tmp_path / "parent/base.yaml").write_text(yaml.safe_dump(document))
    (tmp_path / "spec.yaml").write_text(
        "schema_version: '1.0'\nparents: parent/base.yaml\n"
    )
    prepared = file_owner(tmp_path)
    assert prepared.check_issues() == []
    reads = prepared.capture_reads()
    value, issues = prepared.capture_input_metadata(0, 1024)
    assert value is None
    assert [(row[1], row[2], row[3]) for row in issues] == [
        ("resource_path_missing", "REQ-0785", ["input.SOURCE.path"])
    ]
    assert json.loads(issues[0][4]) == {"dataset": "SOURCE", "path": "input/data.csv"}
    assert prepared.capture_reads() == reads


@pytest.mark.parametrize("absolute", [False, True])
def test_declaring_only_capture_preserves_allowed_parent_and_absolute_paths(
    tmp_path, absolute
):
    directory = tmp_path / "entry"
    directory.mkdir()
    payload = b"ID,VALUE\noriginal,1\n"
    target = tmp_path / "data.csv"
    target.write_bytes(payload)
    document = specification()
    document["input"]["SOURCE"]["path"] = (
        target.as_posix() if absolute else "../data.csv"
    )
    (directory / "spec.yaml").write_text(yaml.safe_dump(document))
    prepared = file_owner(directory)
    assert prepared.check_issues() == []
    assert prepared.capture_input_metadata(0, 1024) == (payload, [])
    assert not (directory / "out.csv").exists()


def test_rejected_normalization_retains_contributor_without_public_context_change(
    tmp_path,
):
    document = specification()
    del document["columns"][0]["type"]
    parent = tmp_path / "parent.yaml"
    parent.write_text(yaml.safe_dump(document))
    entry = tmp_path / "spec.yaml"
    entry.write_text("schema_version: '1.0'\nparents: parent.yaml\n")
    prepared = file_owner(tmp_path)
    assert prepared.preparation_status() == "preparation_failure"
    assert prepared.diagnostic_origins() == [("columns.ID.type", parent.as_posix())]
    rows = prepared.check_issues()
    assert len(rows) == 1
    assert json.loads(rows[0][4]) == {"class": "column_class", "field": "type"}
    import yamaa

    public = yamaa.check(entry)
    assert public.issues["context"].to_list() == [rows[0][4]]
    assert prepared.diagnostic_origins() == [("columns.ID.type", parent.as_posix())]


def test_producer_metadata_uses_native_contract_without_reading_unpublished_output(
    tmp_path,
):
    leaf = specification()
    leaf["output"]["path"] = "leaf.csv"
    (tmp_path / "leaf.yaml").write_text(yaml.safe_dump(leaf))
    root = specification()
    root["input"] = {"PRODUCED": {"path": "leaf.csv", "schema": "./leaf.yaml"}}
    for column in root["columns"]:
        column["derivation"] = column["derivation"].replace("SOURCE.", "PRODUCED.")
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(root))
    prepared = file_owner(tmp_path)
    assert prepared.preparation_status() == "ready", prepared.check_issues()
    reads = prepared.capture_reads()
    assert prepared.check_issues() == []
    identity = (tmp_path / "leaf.yaml").as_posix()
    assert prepared.input_metadata() == [
        (identity, "SOURCE", identity, "absent.csv", "csv")
    ]
    assert prepared.check_source_metadata([(identity, *fields()[0])]) == []
    assert prepared.capture_reads() == reads
    assert not (tmp_path / "leaf.csv").exists()
    assert not (tmp_path / "out.csv").exists()
    for malformed in [
        fields(),
        [(str(tmp_path / "spec.yaml"), *fields()[0])],
        [(identity, "PRODUCED", fields()[0][1])],
        [(identity, *fields()[0]), (identity, *fields()[0])],
    ]:
        with pytest.raises(ValueError):
            prepared.check_source_metadata(malformed)
    assert prepared.check_source_metadata([(identity, *fields()[0])]) == []
    assert prepared.capture_reads() == reads
    with pytest.raises(ValueError, match="ordinary project build capability"):
        prepared.build()


@pytest.mark.parametrize("ordinal", ["VALUE", "POSITION"])
def test_producer_published_schema_alone_owns_consumer_ordinal_collision(
    tmp_path, ordinal
):
    leaf = specification()
    leaf["output"]["path"] = "leaf.csv"
    (tmp_path / "leaf.yaml").write_text(yaml.safe_dump(leaf))
    document = specification()
    document["input"] = {
        "PRODUCED": {"path": "leaf.csv", "schema": "leaf.yaml", "ordinal": ordinal}
    }
    for column in document["columns"]:
        column["derivation"] = column["derivation"].replace("SOURCE.", "PRODUCED.")
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(document))
    prepared = file_owner(tmp_path)
    assert prepared.check_issues() == []
    before = prepared.capture_reads()
    rows = prepared.check_source_metadata(
        [((tmp_path / "leaf.yaml").as_posix(), *fields()[0])]
    )
    assert prepared.capture_reads() == before
    assert not (tmp_path / "leaf.csv").exists()
    if ordinal == "POSITION":
        assert rows == []
    else:
        assert len(rows) == 1
        phase, condition, requirement, paths, context = rows[0]
        assert (phase, condition, requirement, paths) == (
            "validation",
            "ordinal_field_collision",
            "REQ-1294",
            ["input.PRODUCED.ordinal"],
        )
        assert json.loads(context) == {"dataset": "PRODUCED", "field": "VALUE"}


@pytest.mark.parametrize(
    "fixture",
    [
        "negative-output-transport",
        "negative-review-date-text",
        "negative-sum-non-numeric",
        "negative-keys-internal",
        "negative-order-repeated",
        "negative-order-unknown-column",
        "negative-adlb-computed-param",
        "negative-lookup-mismatched",
        "negative-lookup-unknown-key",
    ],
)
def test_original_declaration_and_type_truth_needs_no_cells_or_activation(fixture):
    directory = ROOT / "benchmarks" / fixture
    original = yaml.safe_load((directory / "spec.yaml").read_text())
    truth = yaml.safe_load((directory / "expected/error.yaml").read_text())
    prepared = owner(original)
    # Existing public static Check and original Build checkpoints are unchanged.
    assert json.loads(prepared.check_issues()) == []
    import csv

    metadata = []
    for dataset, source in original["input"].items():
        path = source if isinstance(source, str) else source["path"]
        with (directory / path).open(newline="") as stream:
            names = next(csv.reader(stream))
        metadata.append((dataset, [(name, "str") for name in names]))
    rows = prepared.check_source_metadata(metadata)
    assert len(rows) == 1
    phase, condition, requirement, paths, context = rows[0]
    assert (phase, condition, requirement, paths) == (
        truth["phase"],
        truth["condition"],
        truth["requirement"],
        truth["spec_paths"],
    )
    observed = json.loads(context)
    assert {key: observed[key] for key in truth["context"]} == truth["context"]
    if fixture == "negative-sum-non-numeric":
        assert observed == {
            "actual": "str",
            "expected": "numeric",
            "expr": "SUM(ASEV)",
            "reducer": "SUM",
            "source": "ASEV",
        }
    if fixture == "negative-review-date-text":
        assert observed == {"left_type": "date", "right_type": "str"}


def test_known_operand_type_success_does_not_claim_runtime_value_success():
    original = yaml.safe_load(
        (ROOT / "benchmarks/negative-sum-non-numeric/spec.yaml").read_text()
    )
    original["columns"][3]["type"] = "float"
    prepared = owner(original)
    assert (
        prepared.check_source_metadata(
            [
                (
                    "AE",
                    [
                        (name, "str")
                        for name in ("STUDYID", "USUBJID", "AESEQ", "AESEV")
                    ],
                )
            ]
        )
        == []
    )
    # The original severity words still cannot be converted to float at runtime.
    # The metadata witness has never obtained those authored source values.
