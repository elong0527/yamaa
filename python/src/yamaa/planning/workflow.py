"""Plan and execute acyclic R014 producing-specification workflows."""

from __future__ import annotations

import hashlib
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict

from yamaa.expressions import ExpressionDispatcher
from yamaa.io.artifact import render_artifact
from yamaa.io.project import ProjectResources, ResourceFailure, ResourceSnapshot
from yamaa.io.source import (
    LoadedDataset,
    ProducerContract,
    ProducerField,
    SourceDiagnostic,
    SourceError,
    load_source_tables,
)
from yamaa.runtime.executor import (
    ExecutionHooks,
    ExecutionResult,
    ExecutionSuccess,
    execute_with_source_provider,
)
from yamaa.schema.inheritance import ResolvedSpecification, resolve_specification
from yamaa.specification._yaml import read_yaml_bytes
from yamaa.specification.diagnostics import SpecificationError, ValidationDiagnostic
from yamaa.specification.models import DatasetSource
from yamaa.specification.schema import SchemaBundle


class _FrozenModel(BaseModel):
    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        frozen=True,
        arbitrary_types_allowed=True,
    )


class ProducerLink(_FrozenModel):
    """One consumer dataset and the producer whose artifact it reads."""

    consumer: Path
    dataset: str
    producer: Path
    artifact: Path
    contract: ProducerContract


class WorkflowNode(_FrozenModel):
    """One resolved specification in a producer-first workflow."""

    entry_path: Path
    resolved: ResolvedSpecification
    producers: tuple[Path, ...]


class ProducerWorkflow(_FrozenModel):
    """An acyclic workflow in deterministic producer-first order."""

    entry_path: Path
    nodes: tuple[WorkflowNode, ...]
    links: tuple[ProducerLink, ...]


@dataclass(frozen=True, slots=True)
class WorkflowExecution:
    """The executed prefix, final result, and source snapshots it observed."""

    result: ExecutionResult
    results: Mapping[Path, ExecutionResult]
    sources: Mapping[Path, Mapping[str, LoadedDataset]]
    completed: tuple[Path, ...]


WorkflowEvent = Callable[[Literal["sources", "complete"], Path], None]


def _diagnostic(
    condition: str,
    path: str | Sequence[str],
    requirement: str,
    context: dict[str, object],
) -> ValidationDiagnostic:
    spec_paths = (path,) if isinstance(path, str) else tuple(path)
    return ValidationDiagnostic(
        condition=condition,
        spec_paths=spec_paths,
        requirement=requirement,
        context=context,  # type: ignore[arg-type]
    )


def _physical_path(specification: Path, written: str) -> Path:
    candidate = Path(written)
    if not candidate.is_absolute():
        candidate = specification.parent / candidate
    return candidate.resolve()


def _producer_contract(
    producer: ResolvedSpecification,
    consumer_dataset: str,
) -> ProducerContract:
    specification = producer.specification
    declarations: dict[str, list[object]] = {}
    for column in specification.columns:
        declarations.setdefault(column.name, []).append(column)
    diagnostics: list[ValidationDiagnostic] = []
    fields: list[ProducerField] = []
    seen: set[str] = set()
    if not specification.output.columns:
        diagnostics.append(
            _diagnostic(
                "invalid_producer_contract",
                f"datasets.{consumer_dataset}.schema.output.columns",
                "R014-21",
                {"dataset": consumer_dataset, "reason": "empty"},
            )
        )
    for position, name in enumerate(specification.output.columns):
        if name in seen:
            diagnostics.append(
                _diagnostic(
                    "invalid_producer_contract",
                    f"datasets.{consumer_dataset}.schema.output.columns[{position}]",
                    "R014-21",
                    {
                        "dataset": consumer_dataset,
                        "field": name,
                        "reason": "duplicate",
                    },
                )
            )
            continue
        seen.add(name)
        matches = declarations.get(name, [])
        if len(matches) != 1:
            diagnostics.append(
                _diagnostic(
                    "invalid_producer_contract",
                    f"datasets.{consumer_dataset}.schema.output.columns[{position}]",
                    "R014-21",
                    {
                        "dataset": consumer_dataset,
                        "field": name,
                        "declarations": len(matches),
                    },
                )
            )
            continue
        column = matches[0]
        label = column.label
        if not isinstance(label, str) or not label.strip():
            diagnostics.append(
                _diagnostic(
                    "invalid_producer_contract",
                    f"datasets.{consumer_dataset}.schema.columns.{name}.label",
                    "R014-21",
                    {"dataset": consumer_dataset, "field": name, "reason": "label"},
                )
            )
            continue
        fields.append(ProducerField(name=name, type=column.type, label=label))
    if diagnostics:
        raise SpecificationError(diagnostics)
    return ProducerContract(fields=tuple(fields))


def _schema_snapshot(
    resources: ProjectResources,
    source: DatasetSource,
    dataset: str,
) -> ResourceSnapshot:
    assert source.schema_path is not None
    try:
        snapshot = resources.capture(source.schema_path)
        resources.verify(snapshot)
        return snapshot
    except ResourceFailure as error:
        raise SpecificationError(
            [
                _diagnostic(
                    error.condition,
                    f"datasets.{dataset}.schema",
                    "R014-21",
                    {"dataset": dataset, "path": source.schema_path},
                )
            ]
        ) from error


def plan_workflow(
    entry: str | Path,
    schema_bundle: SchemaBundle,
    resources: ProjectResources,
) -> ProducerWorkflow:
    """Resolve every producer and return one acyclic postorder DAG."""
    entry_path = Path(entry).resolve()
    nodes: dict[Path, WorkflowNode] = {}
    links: list[ProducerLink] = []
    active: list[Path] = []

    def visit(
        path: Path,
        supplied_document: object | None = None,
    ) -> WorkflowNode:
        canonical = path.resolve()
        if canonical in active:
            cycle = active[active.index(canonical) :] + [canonical]
            raise SpecificationError(
                [
                    _diagnostic(
                        "producer_workflow_cycle",
                        "datasets.schema",
                        "R014-21",
                        {"cycle": [str(item) for item in cycle]},
                    )
                ]
            )
        existing = nodes.get(canonical)
        if existing is not None:
            return existing

        resolved = resolve_specification(
            canonical,
            schema_bundle,
            entry_document=supplied_document,
        )
        active.append(canonical)
        producer_paths: list[Path] = []
        node_resources = resources.with_base_directory(canonical.parent)
        for dataset, source in resolved.specification.datasets.items():
            if source.schema_path is None:
                continue
            if "types" in source.model_fields_set:
                declared = source.types or {}
                diagnostics = [
                    _diagnostic(
                        "redundant_field_type",
                        f"datasets.{dataset}.types.{field}",
                        "R014-10",
                        {"dataset": dataset, "field": field, "type": value},
                    )
                    for field, value in declared.items()
                ]
                if not diagnostics:
                    diagnostics.append(
                        _diagnostic(
                            "redundant_field_type",
                            f"datasets.{dataset}.types",
                            "R014-10",
                            {"dataset": dataset, "field": "", "type": ""},
                        )
                    )
                raise SpecificationError(diagnostics)

            snapshot = _schema_snapshot(node_resources, source, dataset)
            producer_path = _physical_path(canonical, source.schema_path)
            producer_document = read_yaml_bytes(snapshot.content, producer_path)
            producer_node = visit(producer_path, producer_document)
            contract = _producer_contract(producer_node.resolved, dataset)
            consumer_artifact = _physical_path(canonical, source.path)
            producer_artifact = _physical_path(
                producer_path,
                producer_node.resolved.specification.output.path,
            )
            if consumer_artifact != producer_artifact:
                raise SpecificationError(
                    [
                        _diagnostic(
                            "producer_output_path_mismatch",
                            (f"datasets.{dataset}.path", f"datasets.{dataset}.schema"),
                            "R014-21",
                            {
                                "dataset": dataset,
                                "source_path": source.path,
                                "output_path": producer_node.resolved.specification.output.path,
                            },
                        )
                    ]
                )
            links.append(
                ProducerLink(
                    consumer=canonical,
                    dataset=dataset,
                    producer=producer_path,
                    artifact=consumer_artifact,
                    contract=contract,
                )
            )
            producer_paths.append(producer_path)
        active.pop()
        node = WorkflowNode(
            entry_path=canonical,
            resolved=resolved,
            producers=tuple(dict.fromkeys(producer_paths)),
        )
        nodes[canonical] = node
        return node

    visit(entry_path)
    return ProducerWorkflow(
        entry_path=entry_path,
        nodes=tuple(nodes.values()),
        links=tuple(links),
    )


def execute_workflow(
    workflow: ProducerWorkflow,
    resources: ProjectResources,
    *,
    dispatcher: ExpressionDispatcher | None = None,
    hooks: ExecutionHooks | None = None,
    event: WorkflowEvent | None = None,
) -> WorkflowExecution:
    """Execute each shared producer once, then its consumers, in memory."""
    selected_dispatcher = dispatcher or ExpressionDispatcher()
    links_by_consumer: dict[Path, dict[str, ProducerLink]] = {}
    for link in workflow.links:
        links_by_consumer.setdefault(link.consumer, {})[link.dataset] = link

    generated: dict[Path, ResourceSnapshot] = {}
    results: dict[Path, ExecutionResult] = {}
    observed_sources: dict[Path, Mapping[str, LoadedDataset]] = {}
    completed: list[Path] = []
    latest: ExecutionResult | None = None

    for node in workflow.nodes:
        node_links = links_by_consumer.get(node.entry_path, {})
        contracts = {dataset: link.contract for dataset, link in node_links.items()}
        snapshots = {
            dataset: generated[link.artifact]
            for dataset, link in node_links.items()
            if link.artifact in generated
        }
        if len(snapshots) != len(node_links):
            missing = sorted(set(node_links) - set(snapshots))
            latest = _source_failure(node.entry_path, missing)
            results[node.entry_path] = latest
            break

        node_sources: dict[str, LoadedDataset] = {}

        def provide(
            datasets: Mapping[str, DatasetSource],
            *,
            base_directory: Path = node.entry_path.parent,
            selected_contracts: Mapping[str, ProducerContract] = contracts,
            selected_snapshots: Mapping[str, ResourceSnapshot] = snapshots,
            captured: dict[str, LoadedDataset] = node_sources,
            node_path: Path = node.entry_path,
        ) -> Mapping[str, LoadedDataset]:
            loaded = load_source_tables(
                datasets,
                resources.with_base_directory(base_directory),
                producer_contracts=selected_contracts,
                producer_snapshots=selected_snapshots,
            )
            captured.update(loaded)
            if event is not None:
                event("sources", node_path)
            return loaded

        latest = execute_with_source_provider(
            node.resolved.specification,
            provide,
            dispatcher=selected_dispatcher,
            hooks=hooks,
        )
        results[node.entry_path] = latest
        observed_sources[node.entry_path] = node_sources
        if not isinstance(latest, ExecutionSuccess):
            break
        completed.append(node.entry_path)
        if event is not None:
            event("complete", node.entry_path)
        content = render_artifact(latest.artifact)
        generated[
            _physical_path(node.entry_path, node.resolved.specification.output.path)
        ] = ResourceSnapshot(
            sha256=hashlib.sha256(content).hexdigest(),
            content=content,
        )

    if latest is None:
        raise ValueError("a producer workflow requires at least one node")
    return WorkflowExecution(
        result=latest,
        results=results,
        sources=observed_sources,
        completed=tuple(completed),
    )


def _source_failure(entry: Path, datasets: Sequence[str]) -> ExecutionResult:
    error = SourceError(
        [
            SourceDiagnostic(
                phase="validation",
                condition="producer_not_completed",
                spec_paths=(f"datasets.{dataset}.schema",),
                requirement="R014-21",
                context={"dataset": dataset, "consumer": str(entry)},
            )
            for dataset in datasets
        ]
    )
    # Keep producer orchestration failures in the same public result envelope as
    # source-provider failures.
    from yamaa.planning.execution import ExecutionDiagnostic
    from yamaa.runtime.executor import ExecutionFailure

    return ExecutionFailure(
        diagnostics=tuple(
            ExecutionDiagnostic(
                phase=item.phase,
                condition=item.condition,
                spec_paths=item.spec_paths,
                requirement=item.requirement,
                context=item.context,
            )
            for item in error.diagnostics
        ),
        handler_counts=(),
    )
