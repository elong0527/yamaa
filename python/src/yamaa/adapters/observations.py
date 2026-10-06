"""Portable execution observations, independent of committed expected artifacts."""

from __future__ import annotations

import datetime
import os
import struct
from dataclasses import replace
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, JsonValue

from yamaa.functions.activation import ActivatedEnvironment
from yamaa.functions.evaluator import function_dispatcher
from yamaa.functions.models import binding_arguments
from yamaa.models import TypedTable
from yamaa.runtime import ExecutionHooks, ExecutionSuccess
from yamaa.verification import check_column, check_dataset


class Observation(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class ScalarObservation(Observation):
    """Exact scalar transport; float bits and i64 values never pass through JSON numbers."""

    type: Literal["missing", "bool", "int", "float", "str", "date", "datetime"]
    value: str | bool | None


def observe_scalar(value: object) -> ScalarObservation:
    if value is None:
        return ScalarObservation(type="missing", value=None)
    if isinstance(value, bool):
        return ScalarObservation(type="bool", value=value)
    if isinstance(value, int):
        return ScalarObservation(type="int", value=str(value))
    if isinstance(value, float):
        return ScalarObservation(type="float", value=struct.pack(">d", value).hex())
    if isinstance(value, datetime.datetime):
        return ScalarObservation(type="datetime", value=value.isoformat())
    if isinstance(value, datetime.date):
        return ScalarObservation(type="date", value=value.isoformat())
    if isinstance(value, str):
        return ScalarObservation(type="str", value=value)
    raise TypeError(f"no portable scalar encoding for {type(value).__name__}")


class TableObservation(Observation):
    specification: str
    stage: Literal["source", "derived"]
    name: str
    columns: tuple[str, ...]
    types: tuple[str, ...]
    rows: tuple[tuple[ScalarObservation, ...], ...]


class CallbackObservation(Observation):
    """One actual study callback; activation vectors and short circuits are excluded."""

    specification: str
    function: str
    contract_version: str
    arguments: tuple[tuple[str, ScalarObservation], ...]


class VerificationObservation(Observation):
    specification: str
    spec_path: str
    check: str
    target: str | None
    requirement: str
    verification_id: str | None
    severity: str
    evaluated_count: int
    # Retain warning details which VerificationFailure.model_dump excludes.
    failure: dict[str, JsonValue] | None


class SourceReadObservation(Observation):
    """One resource capture request during study ingestion, including cached reads."""

    base_directory: str
    path: str
    outcome: Literal["captured", "failure"]
    condition: str | None
    snapshots_created: int


class ObservedResources:
    """Delegate the approved resource port, observing capture without new reads."""

    def __init__(self, resources, observer, base_directory):
        self.resources = resources
        self.observer = observer
        self.base_directory = Path(base_directory)

    def __getattr__(self, name):
        return getattr(self.resources, name)

    def with_base_directory(self, base_directory):
        return ObservedResources(
            self.resources.with_base_directory(base_directory),
            self.observer,
            base_directory,
        )

    def capture(self, written_path):
        before = self.resources.capture_reads
        outcome, condition = "captured", None
        try:
            return self.resources.capture(written_path)
        except BaseException as error:
            outcome, condition = "failure", getattr(error, "condition", None)
            raise
        finally:
            self.observer.source_reads.append(
                SourceReadObservation(
                    base_directory=self.observer.specification_name(
                        self.base_directory
                    ),
                    path=written_path,
                    outcome=outcome,
                    condition=condition,
                    snapshots_created=self.resources.capture_reads - before,
                )
            )


class RunObservations:
    """Observe the ordinary hooks and host boundary without changing evaluation."""

    def __init__(self, entry: Path):
        self.entry = entry
        self.current = self.specification_name(entry)
        self.source_reads: list[SourceReadObservation] = []
        self.callbacks: list[CallbackObservation] = []
        self.verifications: list[VerificationObservation] = []

    def specification_name(self, path: Path) -> str:
        return Path(os.path.relpath(path, self.entry.parent)).as_posix()

    def event(self, event: str, path: Path) -> None:
        if event == "sources":
            self.current = self.specification_name(path)

    def dispatcher(self, activated: ActivatedEnvironment):
        def observed(bound):
            def invoke(**arguments):
                mapping = binding_arguments(bound.contract)
                self.callbacks.append(
                    CallbackObservation(
                        specification=self.current,
                        function=bound.name,
                        contract_version=bound.contract.contract_version,
                        arguments=tuple(
                            (param.name, observe_scalar(arguments[mapping[param.name]]))
                            for param in bound.contract.params
                        ),
                    )
                )
                return bound.target(**arguments)

            return replace(bound, target=invoke)

        functions = {
            name: observed(bound) for name, bound in activated.functions.items()
        }
        return function_dispatcher(replace(activated, functions=functions))

    def _check(self, check, args, records):
        observed = []
        failures = check(*args, records=observed)
        if records is not None:
            records.extend(observed)
        self.record_verifications(observed)
        return failures

    def record_verifications(self, records):
        """Retain complete check evidence supplied by either execution backend."""
        for record in records:
            failure = record.failure
            details = None
            if failure is not None:
                details = failure.model_dump(mode="json")
                details.update(
                    severity=failure.severity,
                    offending_keys=list(failure.offending_keys),
                    log_context=failure.log_context,
                )
            self.verifications.append(
                VerificationObservation(
                    specification=self.current,
                    **record.model_dump(exclude={"failure"}),
                    failure=details,
                )
            )

    def hooks(self) -> ExecutionHooks:
        def column(table, declaration, keys, *, records=None):
            return self._check(check_column, (table, declaration, keys), records)

        def dataset(table, declarations, keys, *, records=None):
            return self._check(check_dataset, (table, declarations, keys), records)

        return ExecutionHooks(column=column, dataset=dataset)

    def table(self, path, stage, name, table: TypedTable) -> TableObservation:
        return TableObservation(
            specification=self.specification_name(path),
            stage=stage,
            name=name,
            columns=tuple(column.name for column in table.columns),
            types=tuple(column.type for column in table.columns),
            rows=tuple(
                tuple(observe_scalar(value) for value in row)
                for row in table.frame.iter_rows()
            ),
        )

    def tables(self, execution) -> tuple[TableObservation, ...]:
        observations = []
        for path, result in execution.results.items():
            for name, source in execution.sources.get(path, {}).items():
                observations.append(self.table(path, "source", name, source.table))
            if isinstance(result, ExecutionSuccess):
                observations.append(self.table(path, "derived", "output", result.table))
        return tuple(observations)
