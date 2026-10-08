"""Compose and publish the study document after its datasets are produced."""

from __future__ import annotations

from collections.abc import Iterable
from pathlib import Path

from yamaa.io import (
    ProducerContract,
    ProducerField,
    ProjectResources,
    approve_roots,
    load_source_table,
)
from yamaa.io.publish import publish_bytes
from yamaa.specification.models import DatasetSource
from yamaa.specification.terminology import load_study_document
from yamaa.submission.composition import compose_study
from yamaa.submission.dataset_json import render_dataset_json
from yamaa.submission.define_xml import render_define_xml
from yamaa.submission.verification import check_terminology
from yamaa.verification import VerificationError


def generate_study_document(
    define_path: str | Path,
    *,
    schema_root: str | Path | None = None,
    project_root: str | Path | None = None,
    data_roots: Iterable[str | Path] | None = None,
    read_project_configuration: bool = True,
) -> Path:
    """Render and validate the entire composition, then publish data and XML.

    Dataset-JSON reads already-published datasets under their producer
    contracts. No specification is re-executed by document generation.
    """
    from yamaa._reference_domain import _discover_schema_root

    entry = Path(define_path).resolve()
    approved = approve_roots(
        entry,
        project_root=project_root,
        data_roots=data_roots,
        read_project_configuration=read_project_configuration,
    )
    resources = ProjectResources(
        approved.project_root,
        base_directory=entry.parent,
        data_roots=approved.data_roots,
    )
    selected_schema = (
        Path(schema_root) if schema_root is not None else _discover_schema_root(entry)
    )
    study = compose_study(load_study_document(entry, selected_schema))
    xml = render_define_xml(study)
    pending: list[tuple[Path, bytes]] = []
    for dataset in study.datasets:
        if not dataset.entry.get("dataset_json"):
            continue
        contract = ProducerContract(
            fields=tuple(
                ProducerField(name=column.name, type=column.type, label=column.label)
                for column in dataset.columns
            )
        )
        source = DatasetSource(path=dataset.specification.output.path)
        table = load_source_table(
            dataset.id,
            source,
            resources.with_base_directory(dataset.specification_path.parent),
            producer_contract=contract,
        ).table
        failures = [
            failure
            for column in dataset.columns
            for failure in check_terminology(
                table,
                column,
                dataset.specification.keys,
                study.document,
                dataset.specification,
            )
        ]
        if failures:
            raise VerificationError(failures)
        pending.append(
            (
                study.output_path.parent / dataset.entry["dataset_json"],
                render_dataset_json(study, dataset, table),
            )
        )
    # Preflight every target before touching the first, including an existing
    # symlink or directory that cannot be replaced by a regular document.
    for target, _ in [*pending, (study.output_path, xml)]:
        if (
            not target.parent.is_dir()
            or target.is_symlink()
            or (target.exists() and not target.is_file())
        ):
            raise ValueError(
                f"a document target must be a regular file in an existing directory: {target}"
            )
    for target, content in pending:
        publish_bytes(target, content)
    return publish_bytes(study.output_path, xml)
