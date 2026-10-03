"""The public runner activates the resolved graph before executing any node."""

from pathlib import Path

import pytest
import yaml

from yamaa.application import prepare_workflow
from yamaa.functions import FunctionActivationError, run_with_project_functions
from yamaa.functions.execution import activate_workflow_functions
from yamaa.planning import execute_workflow
from yamaa.runtime import ExecutionFailure


def write_workflow(root: Path) -> Path:
    (root / "seed.csv").write_text("ID,W\na,1.234\nb,2.345\n")
    (root / "producer.yaml").write_text("""schema_version: "1.0"
domain: PRODUCER
input:
  SEED:
    path: seed.csv
    types: {ID: str, W: float}
base: SEED
keys: [ID]
output: {path: produced.csv, columns: [ID, VALUE], decimals: 2}
columns:
  - {name: ID, type: str, label: Identifier, derivation: SEED.ID}
  - name: VALUE
    type: float
    label: Value
    derivation:
      function:
        name: bmi
        contract_version: "1.0.0"
        args: {weight_kg: SEED.W, height_cm: 100.0}
""")
    entry = root / "spec.yaml"
    entry.write_text("""schema_version: "1.0"
domain: CONSUMER
input:
  A: {path: produced.csv, schema: producer.yaml}
  B: {path: produced.csv, schema: producer.yaml}
base: A
keys: [ID]
output: {path: result.csv, columns: [ID, TOTAL]}
columns:
  - {name: ID, type: str, label: Identifier, derivation: A.ID}
  - {name: AVAL, type: float, label: First value, derivation: A.VALUE}
  - {name: BVAL, type: float, label: Second value, derivation: B.VALUE}
  - name: TOTAL
    type: float
    label: Total
    derivation: {compute: {expr: "AVAL + BVAL"}}
""")
    return entry


def test_producer_only_function_and_rounded_shared_artifact(
    bmi_project, repository, tmp_path, monkeypatch
) -> None:
    from yamaa import application
    from yamaa.functions import execution as functions
    from yamaa.planning import workflow

    entry = write_workflow(tmp_path)
    plans, activations, reads = [], [], []
    plan, activate, load = (
        application.plan_workflow,
        functions.activate,
        workflow.load_source_tables,
    )

    def record_plan(*args, **kwargs):
        plans.append("plan")
        return plan(*args, **kwargs)

    def record_activation(*args, **kwargs):
        assert not reads
        activations.append("activate")
        return activate(*args, **kwargs)

    def record_read(datasets, *args, **kwargs):
        assert activations == ["activate"]
        reads.append(tuple(datasets))
        return load(datasets, *args, **kwargs)

    monkeypatch.setattr(application, "plan_workflow", record_plan)
    monkeypatch.setattr(functions, "activate", record_activation)
    monkeypatch.setattr(workflow, "load_source_tables", record_read)
    run = run_with_project_functions(
        entry, project_root=bmi_project.path, schema_root=repository.schema
    )

    assert run.issues.is_empty(), run.issues.to_dicts()
    assert plans == ["plan"]
    assert reads == [("SEED",), ("A", "B")]
    assert run.output.to_dicts() == [
        {"ID": "a", "TOTAL": 2.46},
        {"ID": "b", "TOTAL": 4.70},
    ]
    assert not (tmp_path / "produced.csv").exists()
    assert not (tmp_path / "result.csv").exists()


@pytest.mark.parametrize("invalid_node", ["producer.yaml", "spec.yaml"])
def test_all_node_calls_are_validated_before_activation_or_sources(
    bmi_project, repository, tmp_path, monkeypatch, invalid_node
) -> None:
    from yamaa.functions import execution as functions
    from yamaa.planning import workflow

    entry = write_workflow(tmp_path)
    path = tmp_path / invalid_node
    document = yaml.safe_load(path.read_text())
    document["columns"][-1]["derivation"] = {
        "function": {
            "name": "bmi",
            "contract_version": "9.0.0",
            "args": {"weight_kg": 1.0, "height_cm": 100.0},
        }
    }
    path.write_text(yaml.safe_dump(document, sort_keys=False))

    def forbidden(*args, **kwargs):
        pytest.fail("invalid calls must prevent activation and all source reads")

    monkeypatch.setattr(functions, "activate", forbidden)
    monkeypatch.setattr(workflow, "load_source_tables", forbidden)
    with pytest.raises(FunctionActivationError) as raised:
        run_with_project_functions(
            entry, project_root=bmi_project.path, schema_root=repository.schema
        )
    assert raised.value.diagnostics[0].condition == "function_contract_mismatch"


def test_failed_producer_never_reads_consumer_sources(
    bmi_project, repository, tmp_path
) -> None:
    from yamaa.functions import function_dispatcher

    entry = write_workflow(tmp_path)
    producer = tmp_path / "producer.yaml"
    document = yaml.safe_load(producer.read_text())
    document["columns"][-1]["verifications"] = [{"range": {"min": 10}}]
    producer.write_text(yaml.safe_dump(document, sort_keys=False))
    prepared = prepare_workflow(entry, schema_root=repository.schema)
    activated = activate_workflow_functions(
        prepared.workflow, bmi_project.path, repository.schema
    )
    events = []
    execution = execute_workflow(
        prepared.workflow,
        prepared.resources,
        dispatcher=function_dispatcher(activated),
        event=lambda event, path: events.append((event, path.name)),
    )
    assert isinstance(execution.result, ExecutionFailure)
    assert events == [("sources", "producer.yaml")]
    assert execution.completed == ()
    assert not (tmp_path / "produced.csv").exists()
