"""Namespace-aware, bounded-memory readers for ODM 1.3 and ODM 2.0."""

from __future__ import annotations

import tarfile
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path, PurePosixPath
from typing import BinaryIO

from lxml import etree
from pydantic import ValidationError
from yamaa.odm.catalog import DefinitionKind, MetadataCatalog
from yamaa.odm.errors import ODMArchiveError, ODMMetadataError, ODMParseError
from yamaa.odm.profiles import (
    ODM_13_NAMESPACE,
    ODM_20_NAMESPACE,
    ImportProfile,
    get_profile,
)
from yamaa.odm.schema import ClinicalItemRow


def _local_name(tag: str) -> str:
    return etree.QName(tag).localname


def _extension_attribute(element: etree._Element, name: str) -> str | None:
    matches = []
    for key, value in element.attrib.items():
        qualified = etree.QName(key)
        if qualified.namespace and qualified.localname == name:
            matches.append(value)
    if len(matches) > 1:
        raise ODMParseError(
            f"multiple namespaced attributes have the local name {name!r}"
        )
    return matches[0] if matches else None


def _required_attribute(element: etree._Element, name: str) -> str:
    value = element.get(name)
    if value is None or value == "":
        raise ODMParseError(f"{_local_name(element.tag)} requires non-empty @{name}")
    return value


def _parse_is_null(value: str | None) -> bool:
    if value is None:
        return False
    normalized = value.casefold()
    if normalized in {"yes", "true", "1"}:
        return True
    if normalized in {"no", "false", "0"}:
        return False
    raise ODMParseError(f"unsupported IsNull value {value!r}")


def _safe_member_name(name: str) -> bool:
    path = PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts


def resolve_archive_member(path: Path, profile: ImportProfile) -> str | None:
    """Select one safe XML archive member, or return None for a plain XML file."""
    is_archive = tarfile.is_tarfile(path)
    archive_suffix = path.name.casefold().endswith((".tar", ".tar.gz", ".tgz"))
    if archive_suffix and not is_archive:
        raise ODMArchiveError(f"cannot read ODM archive {path}")
    if not is_archive:
        if path.stat().st_size > profile.max_expanded_bytes:
            raise ODMArchiveError(
                f"ODM XML is larger than {profile.max_expanded_bytes} bytes"
            )
        return None

    try:
        with tarfile.open(path, "r:*") as archive:
            if profile.archive_member is not None:
                candidates = [
                    member
                    for member in archive.getmembers()
                    if member.name == profile.archive_member
                ]
                if not candidates:
                    raise ODMArchiveError(
                        f"archive does not contain {profile.archive_member!r}"
                    )
                if len(candidates) != 1:
                    raise ODMArchiveError(
                        f"archive contains {len(candidates)} entries named "
                        f"{profile.archive_member!r}"
                    )
            else:
                candidates = [
                    member
                    for member in archive.getmembers()
                    if member.name.casefold().endswith(".xml")
                ]
                if len(candidates) != 1:
                    raise ODMArchiveError(
                        "archive must contain exactly one XML member when the profile "
                        "does not name one"
                    )

            member = candidates[0]
            if not _safe_member_name(member.name):
                raise ODMArchiveError(f"unsafe archive member path {member.name!r}")
            if not member.isfile():
                raise ODMArchiveError(
                    f"archive member {member.name!r} is not a regular file"
                )
            if member.size > profile.max_expanded_bytes:
                raise ODMArchiveError(
                    f"archive member is larger than {profile.max_expanded_bytes} bytes"
                )
            return member.name
    except tarfile.TarError as error:
        raise ODMArchiveError(f"cannot read ODM archive {path}") from error


@contextmanager
def _open_xml_stream(
    path: Path,
    member_name: str | None,
) -> Iterator[BinaryIO]:
    if member_name is None:
        with path.open("rb") as stream:
            yield stream
        return

    try:
        with tarfile.open(path, "r:*") as archive:
            member = archive.getmember(member_name)
            stream = archive.extractfile(member)
            if stream is None:
                raise ODMArchiveError(f"cannot open archive member {member_name!r}")
            with stream:
                yield stream
    except tarfile.TarError as error:
        raise ODMArchiveError(f"cannot read ODM archive {path}") from error


def _release(element: etree._Element) -> None:
    """Release a completed element and already-consumed siblings."""
    element.clear()
    parent = element.getparent()
    if parent is not None:
        while element.getprevious() is not None:
            del parent[0]


def _metadata_name(
    catalog: MetadataCatalog,
    kind: DefinitionKind,
    study_oid: str,
    metadata_version_oid: str,
    oid: str,
    profile: ImportProfile,
) -> tuple[bool, str | None]:
    return catalog.resolve(
        kind,
        study_oid,
        metadata_version_oid,
        oid,
        profile.metadata_aliases,
    )


def _source_or_metadata(source: str | None, metadata: str | None) -> str | None:
    return metadata if source is None else source


def iter_odm_records(
    source: str | Path,
    profile: str | ImportProfile = "strict",
) -> Iterator[ClinicalItemRow]:
    """Yield validated clinical-item rows in XML source order.

    XML parsing is event based and namespace aware. The function never creates a
    CSV representation or retains the complete clinical section in memory.
    """
    source_path = Path(source)
    if not source_path.is_file():
        raise FileNotFoundError(source_path)

    selected_profile = get_profile(profile)
    member_name = resolve_archive_member(source_path, selected_profile)
    catalog = MetadataCatalog()

    metadata_study: str | None = None
    metadata_version: str | None = None
    clinical: dict[str, str] | None = None
    subject: dict[str, str | None] | None = None
    event: dict[str, str | None] | None = None
    form: dict[str, str | None] | None = None
    groups: list[dict[str, str | None]] = []
    odm_namespace: str | None = None
    odm_version: str | None = None
    file_oid: str | None = None
    source_ordinal = 0
    depth = 0

    try:
        with _open_xml_stream(source_path, member_name) as stream:
            context = etree.iterparse(
                stream,
                events=("start", "end"),
                load_dtd=False,
                no_network=True,
                resolve_entities=False,
            )
            for phase, element in context:
                qualified_name = etree.QName(element.tag)
                name = qualified_name.localname
                if phase == "start":
                    depth += 1
                    if depth > selected_profile.max_xml_depth:
                        raise ODMParseError(
                            f"XML depth exceeds {selected_profile.max_xml_depth}"
                        )

                    if depth == 1:
                        if name != "ODM":
                            raise ODMParseError("document root must be ODM")
                        odm_namespace = qualified_name.namespace
                        if odm_namespace not in selected_profile.allowed_namespaces:
                            raise ODMParseError(
                                f"ODM namespace {odm_namespace!r} is not allowed by "
                                f"profile {selected_profile.name!r}"
                            )
                        if element.getroottree().docinfo.doctype:
                            raise ODMParseError(
                                "DOCTYPE declarations are not supported"
                            )
                        expected_version = (
                            "1.3" if odm_namespace == ODM_13_NAMESPACE else "2.0"
                        )
                        odm_version = element.get("ODMVersion") or expected_version
                        if not odm_version.startswith(expected_version):
                            raise ODMParseError(
                                f"ODMVersion {odm_version!r} conflicts with namespace "
                                f"{odm_namespace!r}"
                            )
                        file_oid = element.get("FileOID")
                        continue

                    if qualified_name.namespace != odm_namespace:
                        continue
                    if name == "ODM":
                        raise ODMParseError("document contains a nested ODM element")
                    if clinical is None and name == "Study":
                        metadata_study = _required_attribute(element, "OID")
                    elif clinical is None and name == "MetaDataVersion":
                        if metadata_study is None:
                            raise ODMParseError("MetaDataVersion appears outside Study")
                        metadata_version = _required_attribute(element, "OID")
                    elif clinical is None and name in {
                        "StudyEventDef",
                        "FormDef",
                        "ItemGroupDef",
                        "ItemDef",
                    }:
                        if metadata_study is None or metadata_version is None:
                            raise ODMParseError(
                                f"{name} appears outside MetaDataVersion"
                            )
                        kind: DefinitionKind = {
                            "StudyEventDef": "event",
                            "FormDef": "form",
                            "ItemGroupDef": "group",
                            "ItemDef": "item",
                        }[name]
                        catalog.add(
                            kind,
                            metadata_study,
                            metadata_version,
                            _required_attribute(element, "OID"),
                            element.get("Name"),
                        )
                    elif name == "ClinicalData":
                        if odm_namespace is None:
                            raise ODMParseError(
                                "ClinicalData appears before an ODM root"
                            )
                        clinical = {
                            "StudyOID": _required_attribute(element, "StudyOID"),
                            "MetaDataVersionOID": _required_attribute(
                                element, "MetaDataVersionOID"
                            ),
                        }
                    elif clinical is not None and name == "SubjectData":
                        if subject is not None:
                            raise ODMParseError("nested SubjectData is not supported")
                        subject = {
                            "SubjectKey": _required_attribute(element, "SubjectKey"),
                            "StudySubjectID": _extension_attribute(
                                element, "StudySubjectID"
                            ),
                            "SubjectStatus": _extension_attribute(element, "Status"),
                        }
                    elif clinical is not None and name == "StudyEventData":
                        if event is not None:
                            raise ODMParseError(
                                "nested StudyEventData is not supported"
                            )
                        event = {
                            "StudyEventOID": _required_attribute(
                                element, "StudyEventOID"
                            ),
                            "StudyEventRepeatKey": element.get("StudyEventRepeatKey"),
                            "EventName": _extension_attribute(element, "EventName"),
                            "StartDate": _extension_attribute(element, "StartDate"),
                            "EventStatus": _extension_attribute(element, "Status"),
                            "EventWorkflowStatus": _extension_attribute(
                                element, "WorkflowStatus"
                            ),
                        }
                    elif clinical is not None and name == "FormData":
                        if form is not None:
                            raise ODMParseError("nested FormData is not supported")
                        form = {
                            "FormOID": _required_attribute(element, "FormOID"),
                            "FormRepeatKey": element.get("FormRepeatKey"),
                            "FormName": _extension_attribute(element, "FormName"),
                            "FormLayoutOID": _extension_attribute(
                                element, "FormLayoutOID"
                            ),
                            "FormStatus": _extension_attribute(element, "Status"),
                            "FormWorkflowStatus": _extension_attribute(
                                element, "WorkflowStatus"
                            ),
                            "OpenQueries": _extension_attribute(element, "OpenQueries"),
                        }
                    elif clinical is not None and name == "ItemGroupData":
                        groups.append(
                            {
                                "ItemGroupOID": _required_attribute(
                                    element, "ItemGroupOID"
                                ),
                                "ItemGroupRepeatKey": element.get("ItemGroupRepeatKey"),
                                "ItemGroupName": _extension_attribute(
                                    element, "ItemGroupName"
                                ),
                                "TransactionType": element.get("TransactionType"),
                            }
                        )
                    continue

                is_core_element = qualified_name.namespace == odm_namespace
                if not is_core_element:
                    parent = element.getparent()
                    if parent is None or _local_name(parent.tag) != "ItemData":
                        _release(element)
                    depth -= 1
                    continue

                if clinical is None:
                    if name == "MetaDataVersion":
                        metadata_version = None
                    elif name == "Study":
                        metadata_study = None
                    _release(element)
                    depth -= 1
                    continue

                if name == "ItemData":
                    if subject is None or event is None:
                        raise ODMParseError(
                            "ItemData requires subject and event context"
                        )
                    item_oid = _required_attribute(element, "ItemOID")
                    value_children = [
                        child
                        for child in element
                        if etree.QName(child.tag).namespace == odm_namespace
                        and _local_name(child.tag) == "Value"
                    ]
                    if "Value" in element.attrib and value_children:
                        raise ODMParseError(
                            f"ItemData {item_oid!r} has attribute and child values"
                        )
                    if len(value_children) > 1:
                        raise ODMParseError(
                            f"ItemData {item_oid!r} has multiple Value children"
                        )
                    if "Value" in element.attrib:
                        value = element.get("Value")
                        value_present = True
                    elif value_children:
                        value = value_children[0].text or ""
                        value_present = True
                    else:
                        value = None
                        value_present = False

                    is_null = _parse_is_null(element.get("IsNull"))
                    if is_null and value_present:
                        raise ODMParseError(
                            f"ItemData {item_oid!r} is null and also contains a value"
                        )

                    study_oid = clinical["StudyOID"]
                    version_oid = clinical["MetaDataVersionOID"]
                    if odm_namespace == ODM_13_NAMESPACE:
                        if form is None or len(groups) != 1:
                            raise ODMParseError(
                                "ODM 1.3 ItemData requires one FormData and one "
                                "ItemGroupData ancestor"
                            )
                        projected_form = form
                        item_group = groups[0]
                        form_kind = "form"
                    elif odm_namespace == ODM_20_NAMESPACE:
                        if form is not None or len(groups) != 2:
                            raise ODMParseError(
                                "ODM 2.0 profile requires exactly two nested "
                                "ItemGroupData ancestors and no FormData"
                            )
                        projected_form = {
                            "FormOID": groups[0]["ItemGroupOID"],
                            "FormRepeatKey": groups[0]["ItemGroupRepeatKey"],
                            "FormName": groups[0]["ItemGroupName"],
                            "FormLayoutOID": None,
                            "FormStatus": None,
                            "FormWorkflowStatus": None,
                            "OpenQueries": None,
                        }
                        item_group = groups[1]
                        form_kind = "group"
                    else:
                        raise ODMParseError("ODM root was not initialized")

                    event_oid = str(event["StudyEventOID"])
                    form_oid = str(projected_form["FormOID"])
                    group_oid = str(item_group["ItemGroupOID"])
                    _, event_metadata_name = _metadata_name(
                        catalog,
                        "event",
                        study_oid,
                        version_oid,
                        event_oid,
                        selected_profile,
                    )
                    _, form_metadata_name = _metadata_name(
                        catalog,
                        form_kind,
                        study_oid,
                        version_oid,
                        form_oid,
                        selected_profile,
                    )
                    _, group_metadata_name = _metadata_name(
                        catalog,
                        "group",
                        study_oid,
                        version_oid,
                        group_oid,
                        selected_profile,
                    )
                    item_exists, item_metadata_name = _metadata_name(
                        catalog,
                        "item",
                        study_oid,
                        version_oid,
                        item_oid,
                        selected_profile,
                    )
                    if selected_profile.require_item_metadata and not item_exists:
                        raise ODMMetadataError(
                            "unresolved ItemDef for "
                            f"{study_oid}/{version_oid}/{item_oid} under profile "
                            f"{selected_profile.name!r}"
                        )

                    source_ordinal += 1
                    try:
                        row = ClinicalItemRow.model_validate(
                            {
                                "ODMVersion": odm_version,
                                "FileOID": file_oid,
                                "StudyOID": study_oid,
                                "MetaDataVersionOID": version_oid,
                                "SubjectKey": subject["SubjectKey"],
                                "StudySubjectID": subject["StudySubjectID"],
                                "SubjectStatus": subject["SubjectStatus"],
                                "StudyEventOID": event_oid,
                                "StudyEventRepeatKey": event["StudyEventRepeatKey"],
                                "EventName": _source_or_metadata(
                                    event["EventName"], event_metadata_name
                                ),
                                "StartDate": event["StartDate"],
                                "EventStatus": event["EventStatus"],
                                "EventWorkflowStatus": event["EventWorkflowStatus"],
                                "FormOID": form_oid,
                                "FormRepeatKey": projected_form["FormRepeatKey"],
                                "FormName": _source_or_metadata(
                                    projected_form["FormName"], form_metadata_name
                                ),
                                "FormLayoutOID": projected_form["FormLayoutOID"],
                                "FormStatus": projected_form["FormStatus"],
                                "FormWorkflowStatus": projected_form[
                                    "FormWorkflowStatus"
                                ],
                                "OpenQueries": projected_form["OpenQueries"],
                                "ItemGroupOID": group_oid,
                                "ItemGroupRepeatKey": item_group["ItemGroupRepeatKey"],
                                "ItemGroupName": _source_or_metadata(
                                    item_group["ItemGroupName"], group_metadata_name
                                ),
                                "TransactionType": item_group["TransactionType"],
                                "ItemOID": item_oid,
                                "ItemName": _source_or_metadata(
                                    _extension_attribute(element, "ItemName"),
                                    item_metadata_name,
                                ),
                                "Value": value,
                                "ValuePresent": value_present,
                                "IsNull": is_null,
                                "SourceOrdinal": source_ordinal,
                            }
                        )
                    except ValidationError as error:
                        raise ODMParseError(
                            f"invalid clinical item at source ordinal {source_ordinal}: "
                            f"{error}"
                        ) from error
                    yield row
                    _release(element)
                elif (
                    element.getparent() is not None
                    and _local_name(element.getparent().tag) == "ItemData"
                ):
                    # ItemData consumes and releases all of its direct children.
                    pass
                elif name == "ItemGroupData":
                    if not groups:
                        raise ODMParseError("unbalanced ItemGroupData")
                    groups.pop()
                    _release(element)
                elif name == "FormData":
                    form = None
                    _release(element)
                elif name == "StudyEventData":
                    event = None
                    _release(element)
                elif name == "SubjectData":
                    subject = None
                    _release(element)
                elif name == "ClinicalData":
                    clinical = None
                    _release(element)
                else:
                    _release(element)
                depth -= 1
    except etree.XMLSyntaxError as error:
        raise ODMParseError(f"malformed ODM XML: {error}") from error

    if odm_namespace is None:
        raise ODMParseError("document has no ODM root")
