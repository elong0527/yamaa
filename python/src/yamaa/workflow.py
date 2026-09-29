"""Producing-specification workflows (storage/ingestion).

A dataset declared with `schema` reads the artifact of the specification
`schema` names. That producer is a workflow predecessor: it completes within
the same run before the consumer reads the artifact (REQ-0521), and the
consumer's `path` names the location the producer's `output.path` publishes
to (REQ-1247). Nothing is written: the bytes the producer renders are the
snapshot the consumer reads (REQ-0786), and a producer several datasets
share runs once per run.
"""

import os

from . import compose as _compose
from . import validate as _validate
from .csv_io import read_csv
from .errors import YamaaError


class Workflow:
    """One run's producers: the chain being built, entry first, and each
    completed producer's engine and rendered artifact."""

    def __init__(self, entry_path):
        self.active = [os.path.realpath(entry_path)]
        self.completed = {}


def _fail(where, condition, requirement, context):
    raise YamaaError(
        phase="validation",
        condition=condition,
        requirement=requirement,
        spec_paths=where if isinstance(where, list) else [where],
        context=context,
    )


def _producer_path(engine, name, written):
    """REQ-0520: `schema` is a project path resolved like `path`."""
    written = _compose.layer_path(engine, f"input.{name}.schema", written)
    try:
        _validate.check_resource_path(engine, name, written)
    except YamaaError as e:
        raise YamaaError(
            phase=e.phase,
            condition=e.condition,
            requirement=e.requirement,
            spec_paths=[f"input.{name}.schema"],
            context=e.context,
        ) from None
    return os.path.realpath(os.path.join(engine.spec_dir, written))


def _check_contract(producer, name):
    """REQ-0522: the producer's `output.columns` names each stored field once,
    and each named column supplies the field's type and label."""
    where = f"input.{name}.schema"
    columns = producer.output.get("columns") or []
    if not columns:
        _fail(
            f"{where}.output.columns",
            "producer_contract_mismatch",
            "REQ-0534",
            {"dataset": name, "reason": "empty"},
        )
    for i, column in enumerate(columns):
        if column in columns[:i]:
            _fail(
                f"{where}.output.columns[{i}]",
                "producer_contract_mismatch",
                "REQ-0534",
                {"dataset": name, "field": column, "reason": "duplicate"},
            )
        label = producer.colspecs[column].get("label")
        if not isinstance(label, str) or not label.strip():
            _fail(
                f"{where}.columns.{column}.label",
                "producer_contract_mismatch",
                "REQ-0534",
                {"dataset": name, "field": column, "reason": "label"},
            )


def produced_input(engine, name, decl):
    """(path, types, fields, records) of a dataset another spec produces.

    The producer runs first, under the consumer's project root, and the
    consumer reads its rendered artifact under the producer's column types,
    the one type authority for those fields (REQ-0523, REQ-0524).
    """
    workflow = engine.workflow
    producer_path = _producer_path(engine, name, decl["schema"])
    if producer_path in workflow.active:
        cycle = workflow.active[workflow.active.index(producer_path) :]
        _fail(
            f"input.{name}.schema",
            "dependency_cycle",
            "REQ-0534",
            {"dataset": name, "cycle": cycle + [producer_path]},
        )
    done = workflow.completed.get(producer_path)
    if done is None:
        workflow.active.append(producer_path)
        try:
            producer = type(engine)(
                producer_path, project_root=engine.project_root, workflow=workflow
            )
            _check_contract(producer, name)
            done = (producer, producer.run())
        finally:
            workflow.active.pop()
        workflow.completed[producer_path] = done
    else:
        _check_contract(done[0], name)
    producer, text = done
    path = decl["path"]
    # REQ-1247: the artifact does not exist yet, so `path` names the location
    # of its first anchor, and the producer must publish exactly there.
    location = os.path.realpath(os.path.join(engine.spec_dir, path))
    published = os.path.realpath(
        os.path.join(producer.spec_dir, producer.output.get("path") or "")
    )
    if location != published:
        _fail(
            [f"input.{name}.path", f"input.{name}.schema"],
            "producer_contract_mismatch",
            "REQ-1247",
            {
                "dataset": name,
                "source_path": path,
                "output_path": producer.output.get("path"),
            },
        )
    types = {c: producer.colspecs[c]["type"] for c in producer.output["columns"]}
    fields, records = read_csv(
        location,
        types,
        spec_path=f"input.{name}",
        dataset=name,
        written_path=path,
        raw=text.encode("utf-8"),
    )
    return location, types, fields, records
