"""Plan and execute acyclic R014 producing-specification workflows."""

from __future__ import annotations

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
from yamaa.specification.models import DatasetSource, Specification
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


def _is_mapping_literal(value: object) -> bool:
    return value is None or type(value) in (str, int, float, bool)


def _expand_mapping_dictionary(
    payload: dict[str, object],
    resources: ProjectResources,
    path: str,
) -> None:
    """Replace one mapping dict_yaml with the dictionary it names (REQ-1110).

    The file is read once during workflow planning through the spec's project
    resources, so evaluation never touches the filesystem. Mutates the
    expression payload in place; the payload dicts belong to this plan run.
    """
    written = payload["dict_yaml"]
    if "dict" in payload:
        raise SpecificationError(
            [
                _diagnostic(
                    "mapping_dictionary_source_conflict",
                    path,
                    "REQ-1110",
                    {"fields": ["dict", "dict_yaml"]},
                )
            ]
        )
    if not isinstance(written, str):
        raise SpecificationError(
            [
                _diagnostic(
                    "invalid_field_type",
                    path,
                    "REQ-1110",
                    {"operation": "mapping", "expected": "dict_yaml"},
                )
            ]
        )
    try:
        snapshot = resources.capture(written)
        resources.verify(snapshot)
    except ResourceFailure as error:
        raise SpecificationError(
            [
                _diagnostic(
                    error.condition,
                    path,
                    "REQ-1110",
                    {"field": "dict_yaml", "path": written},
                )
            ]
        ) from error
    try:
        document = read_yaml_bytes(snapshot.content, written)
    except SpecificationError as error:
        relocated = [
            diagnostic.model_copy(update={"spec_paths": (path,)})
            for diagnostic in error.diagnostics
        ]
        raise SpecificationError(relocated) from error
    if not isinstance(document, dict) or any(
        not isinstance(key, str) or not _is_mapping_literal(value)
        for key, value in document.items()
    ):
        raise SpecificationError(
            [
                _diagnostic(
                    "invalid_mapping_dictionary",
                    path,
                    "REQ-1110",
                    {"path": written, "expected": "dict[str, literal_value]"},
                )
            ]
        )
    payload["dict"] = document
    del payload["dict_yaml"]


def _expand_mapping_dictionaries(
    specification: Specification,
    resources: ProjectResources,
) -> None:
    """Expand every mapping dict_yaml in one resolved specification (REQ-1110)."""

    def visit(node: object, path: str) -> None:
        if isinstance(node, BaseModel):
            for name in type(node).model_fields:
                child = f"{path}.{name}" if path else name
                visit(getattr(node, name), child)
        elif isinstance(node, dict):
            payload = node.get("mapping")
            if isinstance(payload, dict) and "dict_yaml" in payload:
                mapping_path = f"{path}.mapping" if path else "mapping"
                _expand_mapping_dictionary(payload, resources, mapping_path)
            for key, item in node.items():
                child = f"{path}.{key}" if path else str(key)
                visit(item, child)
        elif isinstance(node, (list, tuple)):
            for index, item in enumerate(node):
                visit(item, f"{path}[{index}]")

    visit(specification, "")


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
                f"input.{consumer_dataset}.schema.output.columns",
                "REQ-0534",
                {"dataset": consumer_dataset, "reason": "empty"},
            )
        )
    for position, name in enumerate(specification.output.columns):
        if name in seen:
            diagnostics.append(
                _diagnostic(
                    "invalid_producer_contract",
                    f"input.{consumer_dataset}.schema.output.columns[{position}]",
                    "REQ-0534",
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
                    f"input.{consumer_dataset}.schema.output.columns[{position}]",
                    "REQ-0534",
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
                    f"input.{consumer_dataset}.schema.columns.{name}.label",
                    "REQ-0534",
                    {"dataset": consumer_dataset, "field": name, "reason": "label"},
                )
            )
            continue
        fields.append(ProducerField(name=name, type=column.type, label=label))
    if diagnostics:
        raise SpecificationError(diagnostics)
    return ProducerContract(fields=tuple(fields))


def _layer_view(
    resources: ProjectResources,
    resolved: ResolvedSpecification,
    logical: str,
    spelling: str,
) -> tuple[ProjectResources, str]:
    """Resolve a path from the layer that wrote it, as it wrote it (REQ-0780)."""
    origin = resolved.layer_paths.get(logical)
    if origin is None:
        return resources, spelling
    return resources.with_base_directory(origin.directory), origin.written


def _layer_origins(
    resolved: ResolvedSpecification, datasets: Mapping[str, DatasetSource]
) -> dict[str, tuple[Path, str]]:
    """Return where each dataset's path was written and how it was spelled."""
    origins: dict[str, tuple[Path, str]] = {}
    for dataset in datasets:
        origin = resolved.layer_paths.get(f"input.{dataset}.path")
        if origin is not None:
            origins[dataset] = (origin.directory, origin.written)
    return origins


def _schema_snapshot(
    resources: ProjectResources,
    written: str,
    source: DatasetSource,
    dataset: str,
) -> tuple[ResourceSnapshot, Path]:
    assert source.schema_path is not None
    try:
        snapshot = resources.capture(written)
        resources.verify(snapshot)
        # REQ-1246: the producing specification may be found under the
        # project root, so its location is the file the walk reached.
        return snapshot, resources.locate(written)
    except ResourceFailure as error:
        context: dict[str, object] = {"dataset": dataset, "path": source.schema_path}
        if error.condition == "resource_path_missing":
            checked = resources.fallback_root_count(written)
            if checked:
                context["data_roots_checked"] = checked
        raise SpecificationError(
            [
                _diagnostic(
                    error.condition,
                    f"input.{dataset}.schema",
                    "REQ-0534",
                    context,
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
                        "input.schema",
                        "REQ-0534",
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
        _expand_mapping_dictionaries(resolved.specification, node_resources)
        for dataset, source in resolved.specification.input.items():
            if source.schema_path is None:
                continue
            if "types" in source.model_fields_set:
                declared = source.types or {}
                diagnostics = [
                    _diagnostic(
                        "redundant_field_type",
                        f"input.{dataset}.types.{field}",
                        "REQ-0523",
                        {"dataset": dataset, "field": field, "type": value},
                    )
                    for field, value in declared.items()
                ]
                if not diagnostics:
                    diagnostics.append(
                        _diagnostic(
                            "redundant_field_type",
                            f"input.{dataset}.types",
                            "REQ-0523",
                            {"dataset": dataset, "field": "", "type": ""},
                        )
                    )
                raise SpecificationError(diagnostics)

            schema_view, schema_written = _layer_view(
                node_resources, resolved, f"input.{dataset}.schema", source.schema_path
            )
            snapshot, producer_path = _schema_snapshot(
                schema_view, schema_written, source, dataset
            )
            producer_document = read_yaml_bytes(snapshot.content, producer_path)
            producer_node = visit(producer_path, producer_document)
            contract = _producer_contract(producer_node.resolved, dataset)
            path_view, path_written = _layer_view(
                node_resources, resolved, f"input.{dataset}.path", source.path
            )
            try:
                # REQ-1247: the artifact does not exist yet, so its path names
                # the location of its first anchor.
                consumer_artifact = path_view.location(path_written)
            except ResourceFailure as error:
                raise SpecificationError(
                    [
                        _diagnostic(
                            error.condition,
                            f"input.{dataset}.path",
                            "REQ-0534",
                            {"dataset": dataset, "path": source.path},
                        )
                    ]
                ) from error
            producer_artifact = _physical_path(
                producer_path,
                producer_node.resolved.specification.output.path,
            )
            if consumer_artifact != producer_artifact:
                raise SpecificationError(
                    [
                        _diagnostic(
                            "producer_output_path_mismatch",
                            (f"input.{dataset}.path", f"input.{dataset}.schema"),
                            "REQ-0534",
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
            resolved: ResolvedSpecification = node.resolved,
        ) -> Mapping[str, LoadedDataset]:
            loaded = load_source_tables(
                datasets,
                resources.with_base_directory(base_directory),
                producer_contracts=selected_contracts,
                producer_snapshots=selected_snapshots,
                origins=_layer_origins(resolved, datasets),
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
        ] = ResourceSnapshot(content=content)

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
                spec_paths=(f"input.{dataset}.schema",),
                requirement="REQ-0534",
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
