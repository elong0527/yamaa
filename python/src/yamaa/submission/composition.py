"""Resolve a study's submission metadata before any document is rendered."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from yamaa.specification import SpecificationError, ValidationDiagnostic
from yamaa.specification.models import (
    Column,
    DocumentReference,
    Specification,
)
from yamaa.specification.submission import (
    _has_verification,
    _max_length_values,
    _resolve_data_type,
)
from yamaa.specification.terminology import LoadedStudyDocument

_FAMILIES = {
    "SDTMIG": "sdtm",
    "SDTMIG-AP": "sdtm",
    "SDTMIG-MD": "sdtm",
    "SENDIG": "send",
    "SENDIG-AR": "send",
    "SENDIG-DART": "send",
    "SENDIG-GENETOX": "send",
    "ADaMIG": "adam",
    "ADaMIG-MD": "adam",
}
_SDTM_SOURCES = {
    "Collected": {"Subject", "Investigator", "Vendor"},
    "Derived": {"Vendor", "Sponsor"},
    "Assigned": {"Vendor", "Sponsor"},
    "Protocol": {"Sponsor"},
    "Other": {"Subject", "Investigator", "Vendor", "Sponsor"},
    "Predecessor": {None},
    "Not Available": set(),
}


def sas_name(name: str) -> bool:
    return len(name) <= 8 and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) is not None


def reference_id(reference: DocumentReference) -> str:
    return reference if isinstance(reference, str) else reference.document


@dataclass(frozen=True)
class ComposedDataset:
    id: str
    entry: dict[str, Any]
    specification: Specification
    specification_path: Path
    artifact_path: Path
    family: str
    standard: str
    columns: tuple[Column, ...]
    href: str


@dataclass(frozen=True)
class ComposedStudy:
    document: dict[str, Any]
    datasets: tuple[ComposedDataset, ...]
    output_path: Path


def compose_study(study: LoadedStudyDocument) -> ComposedStudy:
    """Apply REQ-0908 and REQ-0959..1025 to one loaded composition.

    Defaults are resolved in copies; source specifications remain reusable
    by another study following a different standard.
    """
    document = study.document
    base = study.written_path.resolve().parent
    # Normalize traversal without following a publication target's symlink.
    output = Path(os.path.abspath(base / document["output"]["path"]))
    diagnostics: list[ValidationDiagnostic] = []
    submission_context = document["context"] == "Submission"

    def fail(condition: str, spec_path: str, requirement: str, **context: Any) -> None:
        diagnostics.append(
            ValidationDiagnostic(
                condition=condition,
                spec_paths=(spec_path,),
                requirement=requirement,
                context=context,
            )
        )

    def unique(entries: list[dict[str, Any]], kind: str) -> dict[str, dict[str, Any]]:
        found: dict[str, dict[str, Any]] = {}
        for entry in entries:
            identifier = entry["id"]
            if identifier in found:
                fail(
                    "duplicate_define_identifier",
                    f"{kind}.{identifier}",
                    "REQ-1017",
                    identifier=identifier,
                )
            found[identifier] = entry
        return found

    standards = unique(document["standards"], "standards")
    documents = unique(document.get("documents") or [], "documents")
    entries = unique(document["datasets"], "datasets")
    for identifier in documents.keys() & entries.keys():
        fail(
            "duplicate_define_identifier",
            f"documents.{identifier}",
            "REQ-1017",
            identifier=identifier,
        )
    acrfs = [
        entry["id"] for entry in documents.values() if entry["kind"] == "annotated_crf"
    ]
    if len(acrfs) > 1:
        fail(
            "value_not_permitted",
            "documents",
            "REQ-1019",
            reason="multiple_annotated_crfs",
        )
    for identifier, standard in standards.items():
        if (standard["type"] == "CT") != (standard.get("publishing_set") is not None):
            fail(
                "value_not_permitted",
                f"standards.{identifier}.publishing_set",
                "REQ-1020",
                standard=identifier,
            )
    if (
        document.get("source_system_version") is not None
        and document.get("source_system") is None
    ):
        fail("source_system_incomplete", "source_system_version", "REQ-0979")
    if output.suffix.lower() != ".xml":
        fail("value_not_permitted", "output.path", "REQ-1014", path=str(output))

    def check_references(references: list[DocumentReference] | None, path: str) -> None:
        for reference in references or []:
            identifier = reference_id(reference)
            if identifier not in documents:
                fail("unknown_document", path, "REQ-1016", document=identifier)

    def check_note(note: Any, path: str) -> None:
        if note is not None and not isinstance(note, str):
            check_references(note.documents, path)

    seen_specs: set[Path] = set()
    seen_artifacts: set[Path] = set()
    json_paths: set[Path] = set()
    composed: list[ComposedDataset] = []
    for entry in document["datasets"]:
        identifier = entry["id"]
        path = f"datasets.{identifier}"
        spec_path = (base / entry["spec"]).resolve()
        spec = study.specifications[identifier]
        artifact = Path(os.path.abspath(spec_path.parent / spec.output.path))
        if spec_path in seen_specs or artifact in seen_artifacts:
            fail("duplicate_dataset_entry", path, "REQ-1018", dataset=identifier)
        seen_specs.add(spec_path)
        seen_artifacts.add(artifact)
        standard_id = entry.get("standard") or document.get("default_standard")
        standard = standards.get(standard_id)
        if standard is None:
            fail(
                "unknown_standard", f"{path}.standard", "REQ-1016", standard=standard_id
            )
            continue
        family = _FAMILIES.get(standard["name"])
        if standard["type"] != "IG" or family is None:
            fail(
                "unknown_standard_family",
                f"{path}.standard",
                "REQ-0924",
                standard=standard_id,
            )
            continue
        if spec.submission is None:
            fail("submission_metadata_missing", path, "REQ-1015", dataset=identifier)
            continue
        if family == "adam" and spec.submission.domain is not None:
            fail(
                "value_not_permitted",
                f"{path}.submission.domain",
                "REQ-0917",
                dataset=identifier,
            )
        check_note(spec.submission.comment, f"{path}.submission.comment")
        if entry.get("has_no_data") and spec.submission.comment is None:
            fail(
                "submission_metadata_missing",
                f"{path}.submission.comment",
                "REQ-1022",
                dataset=identifier,
            )
        if submission_context and not sas_name(identifier):
            fail("sas_name_too_long", path, "REQ-1023", name=identifier)
        target = artifact
        if entry.get("dataset_json") is not None:
            target = Path(os.path.abspath(output.parent / entry["dataset_json"]))
            if target == output or target in json_paths:
                fail(
                    "dataset_json_path_collision",
                    f"{path}.dataset_json",
                    "REQ-1226",
                    dataset=identifier,
                )
            json_paths.add(target)
            if entry.get("has_no_data"):
                fail("dataset_json_without_data", path, "REQ-1227", dataset=identifier)
            if document.get("source_system") and not document.get(
                "source_system_version"
            ):
                fail("source_system_incomplete", "source_system", "REQ-1228")
            if target.parent != output.parent:
                # Both references must stay within their containing directories.
                fail(
                    "artifact_outside_document",
                    f"{path}.dataset_json",
                    "REQ-1229",
                    dataset=identifier,
                    artifact=str(target),
                    document=str(output),
                )
        try:
            href = target.relative_to(output.parent).as_posix()
        except ValueError:
            href = ""
            fail(
                "artifact_outside_document",
                path,
                "REQ-1021",
                dataset=identifier,
                artifact=str(target),
                document=str(output),
            )
        by_name = {column.name: column for column in spec.columns}
        columns: list[Column] = []
        for name in spec.output.columns:
            column = by_name[name]
            column_path = f"{path}.columns.{name}.submission"
            metadata = column.submission
            if metadata is None or not column.label:
                fail(
                    "submission_metadata_missing",
                    column_path,
                    "REQ-1015",
                    dataset=identifier,
                    column=name,
                )
                continue
            if submission_context and not sas_name(name):
                fail("sas_name_too_long", column_path, "REQ-1023", name=name)
            admitted_core = (
                {"Req", "Cond", "Perm"} if family == "adam" else {"Req", "Exp", "Perm"}
            )
            if (
                metadata.core not in admitted_core
                or (family == "adam" and metadata.mandatory is None)
                or (
                    family != "adam"
                    and metadata.core == "Req"
                    and metadata.mandatory is False
                )
            ):
                fail(
                    "core_mandatory_conflict",
                    column_path,
                    "REQ-0915",
                    dataset=identifier,
                    column=name,
                    family=family,
                )
            mandatory = metadata.mandatory
            if mandatory is None:
                mandatory = metadata.core == "Req"
            if (
                mandatory
                and name not in spec.keys
                and not _has_verification(column.verifications, "not_missing")
            ):
                fail("mandatory_not_enforced", column_path, "REQ-0916", column=name)
            if family == "adam" and metadata.role is not None:
                fail(
                    "value_not_permitted",
                    column_path + ".role",
                    "REQ-0917",
                    column=name,
                )
            origin = metadata.origin
            assert origin is not None  # Specification validation requires this.
            if family == "sdtm":
                admitted = origin.source in _SDTM_SOURCES[origin.type]
            elif family == "send":
                admitted = origin.source is None
            else:
                admitted = (
                    origin.type in {"Derived", "Assigned", "Predecessor", "Other"}
                    and origin.source is None
                )
            if not admitted:
                fail(
                    "origin_source_not_admitted",
                    column_path + ".origin",
                    "REQ-0919",
                    column=name,
                    family=family,
                    origin_type=origin.type,
                    source=origin.source,
                )
            references = origin.documents
            if origin.type == "Collected" and origin.source in {
                "Investigator",
                "Subject",
            }:
                if len(acrfs) != 1:
                    fail(
                        "origin_document_missing",
                        column_path + ".origin",
                        "REQ-0984",
                        column=name,
                    )
                elif references is None:
                    references = [acrfs[0]]
                elif not references or any(
                    reference_id(ref) != acrfs[0] for ref in references
                ):
                    fail(
                        "origin_document_missing",
                        column_path + ".origin.documents",
                        "REQ-0893",
                        column=name,
                    )
            check_references(references, column_path + ".origin.documents")
            check_note(metadata.method, column_path + ".method")
            check_note(metadata.comment, column_path + ".comment")
            origin = origin.model_copy(
                update={
                    "documents": references,
                    "source": (None if origin.type == "Predecessor" else "Sponsor")
                    if family == "adam"
                    else origin.source,
                }
            )
            data_type = _resolve_data_type(column.type, metadata.data_type)
            maximum = _max_length_values(column.verifications)
            length = metadata.length
            if (
                length is None
                and column.type == "str"
                and data_type in {"text", "integer", "float"}
                and maximum
            ):
                length = maximum[0]
            if (
                data_type not in {"text", "integer", "float"}
                and metadata.length is not None
            ):
                fail(
                    "submission_length_not_applicable",
                    column_path + ".length",
                    "REQ-0912",
                    column=name,
                    data_type=data_type,
                )
            resolved = metadata.model_copy(
                update={
                    "mandatory": mandatory,
                    "origin": origin,
                    "data_type": data_type,
                    "length": length,
                }
            )
            columns.append(column.model_copy(update={"submission": resolved}))
        for row in spec.rows or []:
            if submission_context:
                for name in row.submission or {}:
                    fail(
                        "prohibited_construct",
                        f"{path}.rows.{row.id}.submission.{name}",
                        "REQ-1169",
                        dataset=identifier,
                        row=row.id,
                        column=name,
                    )
        composed.append(
            ComposedDataset(
                identifier,
                entry,
                spec,
                spec_path,
                artifact,
                family,
                standard_id,
                tuple(columns),
                href,
            )
        )
    for target in json_paths & seen_artifacts:
        fail("dataset_json_path_collision", "datasets", "REQ-1226", path=str(target))
    if output in seen_artifacts:
        fail("artifact_path_collision", "output.path", "REQ-1195", path=str(output))
    if submission_context:
        for codelist in document.get("codelists") or []:
            standard = standards.get(codelist.get("standard"), {})
            if (
                codelist.get("items") is not None
                and standard.get("name") == "CDISC/NCI"
                and (
                    not codelist.get("alias")
                    or any(not item.get("alias") for item in codelist["items"])
                )
            ):
                fail(
                    "missing_required_field",
                    f"codelists.{codelist['id']}",
                    "REQ-1004",
                    field="alias",
                )
    generated_comments: set[str] = set()
    for dataset in composed:
        assert dataset.specification.submission is not None
        comments = [(dataset.id, dataset.specification.submission.comment)]
        comments.extend(
            (f"{dataset.id}.{column.name}", column.submission.comment)
            for column in dataset.columns
            if column.submission is not None
        )
        for identifier, comment in comments:
            if comment is None:
                continue
            if identifier in generated_comments:
                fail(
                    "duplicate_define_identifier",
                    f"datasets.{dataset.id}",
                    "REQ-0971",
                    identifier="COM." + identifier,
                )
            generated_comments.add(identifier)
    if diagnostics:
        raise SpecificationError(diagnostics)
    return ComposedStudy(document, tuple(composed), output)
