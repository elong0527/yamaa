from __future__ import annotations

from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
import pytest

from yamaa.io import (
    ProducerContract,
    ProducerField,
    ProjectResources,
    SourceError,
    load_source_table,
)
from yamaa.planning import execute_workflow, plan_workflow
from yamaa.runtime import ExecutionSuccess
from yamaa.specification import SpecificationError
from yamaa.specification.models import DatasetSource
from yamaa.specification.schema import load_schema_bundle

REPOSITORY = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY / "yaml"
EXAMPLES = REPOSITORY / "benchmarks"


def test_two_stage_workflow_completes_producer_before_consumer_ingestion() -> None:
    example = EXAMPLES / "adam-adsl-randomization"
    resources = ProjectResources(example)
    workflow = plan_workflow(
        example / "spec.yaml", load_schema_bundle(SCHEMA_ROOT), resources
    )
    events: list[tuple[str, str]] = []

    execution = execute_workflow(
        workflow,
        resources,
        event=lambda event, path: events.append((event, path.name)),
    )

    assert isinstance(execution.result, ExecutionSuccess)
    assert execution.result.artifact.frame.columns == [
        "STUDYID",
        "USUBJID",
        "RANDDT",
        "RANDDY",
        "RANDDTC",
    ]
    producer = (example / "input/dm.schema.yaml").resolve()
    consumer = (example / "spec.yaml").resolve()
    assert execution.completed == (producer, consumer)
    assert events.index(("complete", "dm.schema.yaml")) < events.index(
        ("sources", "spec.yaml")
    )


@pytest.mark.parametrize(
    ("content", "missing", "extra", "reordered"),
    [
        (b"AGE,ID\n42,01\n", [], [], True),
        (b"ID\n01\n", ["AGE"], [], False),
        (b"ID,AGE,OTHER\n01,42,x\n", [], ["OTHER"], False),
    ],
)
def test_producer_contract_rejects_reordered_missing_and_extra_headers(
    tmp_path: Path,
    content: bytes,
    missing: list[str],
    extra: list[str],
    reordered: bool,
) -> None:
    (tmp_path / "artifact.csv").write_bytes(content)
    source = DatasetSource(path="artifact.csv", schema="producer.yaml")
    contract = ProducerContract(
        fields=(
            ProducerField(name="ID", type="str", label="Identifier"),
            ProducerField(name="AGE", type="int", label="Age"),
        )
    )

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM",
            source,
            ProjectResources(tmp_path),
            producer_contract=contract,
        )

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "producer_contract_mismatch"
    assert diagnostic.requirement == "REQ-0535"
    assert diagnostic.context == {
        "dataset": "DM",
        "expected": ["ID", "AGE"],
        "actual": content.splitlines()[0].decode("ascii").split(","),
        "missing": missing,
        "extra": extra,
        "reordered": reordered,
    }


def test_producer_contract_rejects_a_parquet_type_mismatch(
    tmp_path: Path,
) -> None:
    pq.write_table(
        pa.table(
            {
                "ID": pa.array(["01"], type=pa.string()),
                "AGE": pa.array(["42"], type=pa.string()),
            }
        ),
        tmp_path / "artifact.parquet",
    )
    source = DatasetSource(path="artifact.parquet", schema="producer.yaml")
    contract = ProducerContract(
        fields=(
            ProducerField(name="ID", type="str", label="Identifier"),
            ProducerField(name="AGE", type="int", label="Age"),
        )
    )

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM",
            source,
            ProjectResources(tmp_path),
            producer_contract=contract,
        )

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "producer_contract_mismatch"
    assert diagnostic.requirement == "REQ-0535"
    assert diagnostic.context == {
        "dataset": "DM",
        "field": "AGE",
        "expected_type": "int",
        "actual_type": "str",
    }


def test_shared_producer_executes_once_and_shares_generated_snapshot(
    tmp_path: Path,
) -> None:
    (tmp_path / "seed.csv").write_text("ID\n01\n", encoding="ascii")
    (tmp_path / "producer.yaml").write_text(
        """schema_version: "1.0"
domain: PRODUCED
input: {SEED: seed.csv}
base: SEED
keys: [ID]
output: {path: produced.parquet, columns: [ID]}
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: SEED.ID}
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
domain: OUT
input:
  A: {path: produced.parquet, schema: producer.yaml}
  B: {path: produced.parquet, schema: producer.yaml}
base: A
keys: [ID]
output: {path: out.csv, columns: [ID]}
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: A.ID}
""",
        encoding="ascii",
    )
    resources = ProjectResources(tmp_path)
    workflow = plan_workflow(
        tmp_path / "spec.yaml", load_schema_bundle(SCHEMA_ROOT), resources
    )
    completed: list[Path] = []

    execution = execute_workflow(
        workflow,
        resources,
        event=lambda event, path: (
            completed.append(path) if event == "complete" else None
        ),
    )

    producer = (tmp_path / "producer.yaml").resolve()
    assert [node.entry_path for node in workflow.nodes].count(producer) == 1
    assert completed.count(producer) == 1
    assert isinstance(execution.result, ExecutionSuccess)
    assert execution.result.artifact.frame.to_dicts() == [{"ID": "01"}]
    assert not (tmp_path / "produced.parquet").exists()
    consumer_sources = execution.sources[(tmp_path / "spec.yaml").resolve()]
    assert consumer_sources["A"].snapshot is consumer_sources["B"].snapshot
    # One producer schema plus the producer's external seed; generated bytes
    # enter the consumer without another filesystem read.
    assert resources.capture_reads == 2


def test_producer_cycle_fails_before_execution(tmp_path: Path) -> None:
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
domain: A
input: {B: {path: b.csv, schema: producer.yaml}}
base: B
keys: [ID]
output: {path: a.csv, columns: [ID]}
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: B.ID}
""",
        encoding="ascii",
    )
    (tmp_path / "producer.yaml").write_text(
        """schema_version: "1.0"
domain: B
input: {A: {path: a.csv, schema: spec.yaml}}
base: A
keys: [ID]
output: {path: b.csv, columns: [ID]}
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: A.ID}
""",
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as raised:
        plan_workflow(
            tmp_path / "spec.yaml",
            load_schema_bundle(SCHEMA_ROOT),
            ProjectResources(tmp_path),
        )

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "producer_workflow_cycle"
    assert diagnostic.requirement == "REQ-0534"


def test_inline_types_conflict_with_producer_before_sources_are_read() -> None:
    example = EXAMPLES / "negative-redefined-date"
    resources = ProjectResources(example)

    with pytest.raises(SpecificationError) as raised:
        plan_workflow(example / "spec.yaml", load_schema_bundle(SCHEMA_ROOT), resources)

    assert raised.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "redundant_field_type",
        "spec_paths": ["input.DM.types.RANDDT"],
        "requirement": "REQ-0523",
        "context": {"dataset": "DM", "field": "RANDDT", "type": "date"},
    }
    assert resources.capture_reads == 0


def _write_mapping_project(
    tmp_path: Path, mapping_payload: str, dictionary: str | None
) -> None:
    (tmp_path / "input").mkdir()
    (tmp_path / "input" / "ae.csv").write_text(
        "USUBJID,AESEQ,AESEV\nP7-501,1,MILD\nP7-501,2,SEVERE\n", encoding="ascii"
    )
    if dictionary is not None:
        (tmp_path / "sevord.yaml").write_text(dictionary, encoding="ascii")
    (tmp_path / "spec.yaml").write_text(
        'schema_version: "1.0"\n'
        "domain: ADAE\n"
        "keys: [USUBJID, ASEQ]\n"
        "input:\n"
        "  AE: input/ae.csv\n"
        "output:\n"
        "  path: adae.csv\n"
        "  columns: [USUBJID, ASEQ, SEVORD]\n"
        "columns:\n"
        "  - name: USUBJID\n"
        "    type: str\n"
        "    derivation: AE.USUBJID\n"
        "  - name: ASEQ\n"
        "    type: int\n"
        "    derivation: AE.AESEQ\n"
        "  - name: ASEV\n"
        "    type: str\n"
        "    derivation: AE.AESEV\n"
        "  - name: SEVORD\n"
        "    type: int\n"
        "    derivation:\n"
        f"      mapping:\n{mapping_payload}",
        encoding="ascii",
    )


def test_mapping_dict_yaml_loads_dictionary_from_file(tmp_path: Path) -> None:
    _write_mapping_project(
        tmp_path,
        "        source: ASEV\n        missing: null\n        dict_yaml: sevord.yaml\n",
        "MILD: 1\nMODERATE: 2\nSEVERE: 3\n",
    )
    resources = ProjectResources(tmp_path)
    workflow = plan_workflow(
        tmp_path / "spec.yaml", load_schema_bundle(SCHEMA_ROOT), resources
    )
    execution = execute_workflow(workflow, resources)

    assert isinstance(execution.result, ExecutionSuccess)
    frame = execution.result.artifact.frame
    assert frame.columns == ["USUBJID", "ASEQ", "SEVORD"]
    assert frame["SEVORD"].to_list() == [1, 3]


def test_mapping_dict_yaml_rejects_inline_dict_beside_it(tmp_path: Path) -> None:
    _write_mapping_project(
        tmp_path,
        "        source: ASEV\n        dict: {MILD: 1}\n        dict_yaml: sevord.yaml\n",
        "MILD: 1\n",
    )

    with pytest.raises(SpecificationError) as raised:
        plan_workflow(
            tmp_path / "spec.yaml",
            load_schema_bundle(SCHEMA_ROOT),
            ProjectResources(tmp_path),
        )

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "mapping_dictionary_source_conflict"
    assert diagnostic.requirement == "REQ-1110"
    assert diagnostic.spec_paths[0] == "columns[3].derivation.value.root.mapping"


def test_mapping_dict_yaml_missing_file_is_a_plan_error(tmp_path: Path) -> None:
    _write_mapping_project(
        tmp_path,
        "        source: ASEV\n        dict_yaml: sevord.yaml\n",
        None,
    )

    with pytest.raises(SpecificationError) as raised:
        plan_workflow(
            tmp_path / "spec.yaml",
            load_schema_bundle(SCHEMA_ROOT),
            ProjectResources(tmp_path),
        )

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "resource_path_missing"
    assert diagnostic.context["path"] == "sevord.yaml"


def test_mapping_dict_yaml_rejects_paths_outside_the_project(tmp_path: Path) -> None:
    _write_mapping_project(
        tmp_path,
        "        source: ASEV\n        dict_yaml: ../escape.yaml\n",
        None,
    )

    with pytest.raises(SpecificationError) as raised:
        plan_workflow(
            tmp_path / "spec.yaml",
            load_schema_bundle(SCHEMA_ROOT),
            ProjectResources(tmp_path),
        )

    assert raised.value.diagnostics[0].condition == "resource_path_outside_project"


@pytest.mark.parametrize(
    "dictionary",
    ["- MILD\n- SEVERE\n", "1: one\n"],
)
def test_mapping_dict_yaml_rejects_non_mapping_content(
    tmp_path: Path, dictionary: str
) -> None:
    _write_mapping_project(
        tmp_path,
        "        source: ASEV\n        dict_yaml: sevord.yaml\n",
        dictionary,
    )

    with pytest.raises(SpecificationError) as raised:
        plan_workflow(
            tmp_path / "spec.yaml",
            load_schema_bundle(SCHEMA_ROOT),
            ProjectResources(tmp_path),
        )

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "invalid_mapping_dictionary"
    assert diagnostic.requirement == "REQ-1110"
