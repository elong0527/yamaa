"""General namespace-aware readers for ODM clinical-item data."""

from __future__ import annotations

import tarfile
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path, PurePosixPath
from typing import BinaryIO, Literal

import polars as pl
from lxml import etree
from pydantic import ValidationError

from yamaa.odm.errors import ODMError
from yamaa.odm.schema import ClinicalItemRow, rows_to_frame

ODM_13_NAMESPACE = "http://www.cdisc.org/ns/odm/v1.3"
ODM_20_NAMESPACE = "http://www.cdisc.org/ns/odm/v2.0"
SUPPORTED_NAMESPACES = frozenset({ODM_13_NAMESPACE, ODM_20_NAMESPACE})

DefinitionKind = Literal["event", "form", "group", "item"]
MetadataKey = tuple[str, str, str]


def _local_name(tag: str) -> str:
    return etree.QName(tag).localname


def _extension_attribute(element: etree._Element, name: str) -> str | None:
    matches = []
    for key, value in element.attrib.items():
        qualified = etree.QName(key)
        if qualified.namespace and qualified.localname == name:
            matches.append(value)
    if len(matches) > 1:
        raise ODMError(f"multiple namespaced attributes have local name {name!r}")
    return matches[0] if matches else None


def _required_attribute(element: etree._Element, name: str) -> str:
    value = element.get(name)
    if value is None or value == "":
        raise ODMError(f"{_local_name(element.tag)} requires non-empty @{name}")
    return value


def _parse_is_null(value: str | None) -> bool:
    if value is None:
        return False
    normalized = value.casefold()
    if normalized in {"yes", "true", "1"}:
        return True
    if normalized in {"no", "false", "0"}:
        return False
    raise ODMError(f"unsupported IsNull value {value!r}")


def _source_or_metadata(source: str | None, metadata: str | None) -> str | None:
    return metadata if source is None else source


def _safe_member_name(name: str) -> bool:
    path = PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts


def _is_os_metadata(name: str) -> bool:
    path = PurePosixPath(name)
    return path.name.startswith("._") or "__MACOSX__" in path.parts


def _bounded_archive_members(
    archive: tarfile.TarFile,
    max_archive_members: int,
) -> Iterator[tarfile.TarInfo]:
    for member_count, member in enumerate(archive, start=1):
        if member_count > max_archive_members:
            raise ODMError(
                f"TAR input contains more than {max_archive_members} members"
            )
        yield member


def resolve_archive_member(
    path: Path,
    archive_member: str | None,
    max_expanded_bytes: int,
    max_archive_members: int = 10_000,
) -> str | None:
    """Select one safe XML archive member, or return None for plain XML."""
    is_archive = tarfile.is_tarfile(path)
    archive_suffix = path.name.casefold().endswith((".tar", ".tar.gz", ".tgz"))
    if archive_suffix and not is_archive:
        raise ODMError(f"cannot read ODM archive {path}")
    if not is_archive:
        if archive_member is not None:
            raise ODMError("archive_member requires a TAR input")
        if path.stat().st_size > max_expanded_bytes:
            raise ODMError(f"ODM XML exceeds {max_expanded_bytes} bytes")
        return None

    try:
        with tarfile.open(path, "r:*") as archive:
            selected_member: tarfile.TarInfo | None = None
            candidate_count = 0
            for member in _bounded_archive_members(archive, max_archive_members):
                if archive_member is None:
                    is_candidate = member.name.casefold().endswith(
                        ".xml"
                    ) and not _is_os_metadata(member.name)
                else:
                    is_candidate = member.name == archive_member
                if is_candidate:
                    candidate_count += 1
                    if selected_member is None:
                        selected_member = member

            if archive_member is None:
                if candidate_count != 1:
                    raise ODMError(
                        "TAR input must contain exactly one XML member or receive "
                        "archive_member"
                    )
            else:
                if candidate_count != 1:
                    raise ODMError(
                        f"TAR input contains {candidate_count} entries named "
                        f"{archive_member!r}"
                    )

            assert selected_member is not None
            member = selected_member
            if not _safe_member_name(member.name):
                raise ODMError(f"unsafe TAR member path {member.name!r}")
            if not member.isfile():
                raise ODMError(f"TAR member {member.name!r} is not a regular file")
            if member.size > max_expanded_bytes:
                raise ODMError(f"TAR member exceeds {max_expanded_bytes} bytes")
            return member.name
    except tarfile.TarError as error:
        raise ODMError(f"cannot read ODM archive {path}") from error


@contextmanager
def _open_xml_stream(
    path: Path,
    member_name: str | None,
    max_archive_members: int,
) -> Iterator[BinaryIO]:
    if member_name is None:
        with path.open("rb") as stream:
            yield stream
        return

    try:
        with tarfile.open(path, "r:*") as archive:
            member = None
            for candidate in _bounded_archive_members(archive, max_archive_members):
                if candidate.name == member_name:
                    member = candidate
                    break
            if member is None:
                raise ODMError(f"cannot open TAR member {member_name!r}")
            stream = archive.extractfile(member)
            if stream is None:
                raise ODMError(f"cannot open TAR member {member_name!r}")
            with stream:
                yield stream
    except tarfile.TarError as error:
        raise ODMError(f"cannot read ODM archive {path}") from error


def _release(element: etree._Element) -> None:
    element.clear()
    parent = element.getparent()
    if parent is not None:
        while element.getprevious() is not None:
            del parent[0]


def _add_metadata_name(
    names: dict[DefinitionKind, dict[MetadataKey, str | None]],
    kind: DefinitionKind,
    study_oid: str,
    version_oid: str,
    oid: str,
    name: str | None,
) -> None:
    key = (study_oid, version_oid, oid)
    existing = names[kind].get(key)
    if key in names[kind] and existing != name:
        raise ODMError(f"conflicting {kind} definition for {'/'.join(key)}")
    names[kind][key] = name


def _metadata_name(
    names: dict[DefinitionKind, dict[MetadataKey, str | None]],
    kind: DefinitionKind,
    study_oid: str,
    version_oid: str,
    oid: str,
) -> str | None:
    return names[kind].get((study_oid, version_oid, oid))


def iter_odm_records(
    source: str | Path,
    *,
    archive_member: str | None = None,
    max_archive_members: int = 10_000,
    max_expanded_bytes: int = 256 * 1024 * 1024,
    max_xml_depth: int = 64,
) -> Iterator[ClinicalItemRow]:
    """Yield one validated row per clinical item in source order.

    TAR inputs may contain at most max_archive_members entries.
    A direct FormData ancestor supplies form context. When FormData is absent,
    exactly two nested ItemGroupData ancestors are supported: the outer group
    supplies form context and the inner group supplies item-group context.
    """
    source_path = Path(source)
    if not source_path.is_file():
        raise FileNotFoundError(source_path)
    if max_expanded_bytes < 1:
        raise ODMError("max_expanded_bytes must be at least 1")
    if max_archive_members < 1:
        raise ODMError("max_archive_members must be at least 1")
    if max_xml_depth < 8:
        raise ODMError("max_xml_depth must be at least 8")

    member_name = resolve_archive_member(
        source_path,
        archive_member,
        max_expanded_bytes,
        max_archive_members,
    )
    names: dict[DefinitionKind, dict[MetadataKey, str | None]] = {
        "event": {},
        "form": {},
        "group": {},
        "item": {},
    }

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
        with _open_xml_stream(
            source_path,
            member_name,
            max_archive_members,
        ) as stream:
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
                    if depth > max_xml_depth:
                        raise ODMError(f"XML depth exceeds {max_xml_depth}")

                    if depth == 1:
                        if name != "ODM":
                            raise ODMError("document root must be ODM")
                        odm_namespace = qualified_name.namespace
                        if odm_namespace not in SUPPORTED_NAMESPACES:
                            raise ODMError(
                                f"unsupported ODM namespace {odm_namespace!r}"
                            )
                        if element.getroottree().docinfo.doctype:
                            raise ODMError("DOCTYPE declarations are not supported")
                        expected_version = (
                            "1.3" if odm_namespace == ODM_13_NAMESPACE else "2.0"
                        )
                        odm_version = element.get("ODMVersion") or expected_version
                        if not odm_version.startswith(expected_version):
                            raise ODMError(
                                f"ODMVersion {odm_version!r} conflicts with namespace"
                            )
                        file_oid = element.get("FileOID")
                        continue

                    if qualified_name.namespace != odm_namespace:
                        continue
                    if name == "ODM":
                        raise ODMError("document contains a nested ODM element")
                    if clinical is None and name == "Study":
                        metadata_study = _required_attribute(element, "OID")
                    elif clinical is None and name == "MetaDataVersion":
                        if metadata_study is None:
                            raise ODMError("MetaDataVersion appears outside Study")
                        metadata_version = _required_attribute(element, "OID")
                    elif clinical is None and name in {
                        "StudyEventDef",
                        "FormDef",
                        "ItemGroupDef",
                        "ItemDef",
                    }:
                        if metadata_study is None or metadata_version is None:
                            raise ODMError(f"{name} appears outside MetaDataVersion")
                        kind: DefinitionKind = {
                            "StudyEventDef": "event",
                            "FormDef": "form",
                            "ItemGroupDef": "group",
                            "ItemDef": "item",
                        }[name]
                        _add_metadata_name(
                            names,
                            kind,
                            metadata_study,
                            metadata_version,
                            _required_attribute(element, "OID"),
                            element.get("Name"),
                        )
                    elif name == "ClinicalData":
                        clinical = {
                            "StudyOID": _required_attribute(element, "StudyOID"),
                            "MetaDataVersionOID": _required_attribute(
                                element, "MetaDataVersionOID"
                            ),
                        }
                    elif clinical is not None and name == "SubjectData":
                        if subject is not None:
                            raise ODMError("nested SubjectData is not supported")
                        subject = {
                            "SubjectKey": _required_attribute(element, "SubjectKey"),
                            "StudySubjectID": _extension_attribute(
                                element, "StudySubjectID"
                            ),
                            "SubjectStatus": _extension_attribute(element, "Status"),
                        }
                    elif clinical is not None and name == "StudyEventData":
                        if event is not None:
                            raise ODMError("nested StudyEventData is not supported")
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
                            raise ODMError("nested FormData is not supported")
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
                        raise ODMError("ItemData requires subject and event context")
                    item_oid = _required_attribute(element, "ItemOID")
                    value_children = [
                        child
                        for child in element
                        if etree.QName(child.tag).namespace == odm_namespace
                        and _local_name(child.tag) == "Value"
                    ]
                    if "Value" in element.attrib and value_children:
                        raise ODMError(
                            f"ItemData {item_oid!r} has attribute and child values"
                        )
                    if len(value_children) > 1:
                        raise ODMError(
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
                        raise ODMError(
                            f"ItemData {item_oid!r} is null and contains a value"
                        )

                    study_oid = clinical["StudyOID"]
                    version_oid = clinical["MetaDataVersionOID"]
                    if form is not None and len(groups) == 1:
                        projected_form = form
                        item_group = groups[0]
                        form_kind: DefinitionKind = "form"
                    elif form is None and len(groups) == 2:
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
                        raise ODMError(
                            "ItemData requires FormData plus one ItemGroupData, or "
                            "two ItemGroupData ancestors without FormData"
                        )

                    event_oid = str(event["StudyEventOID"])
                    form_oid = str(projected_form["FormOID"])
                    group_oid = str(item_group["ItemGroupOID"])
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
                                    event["EventName"],
                                    _metadata_name(
                                        names,
                                        "event",
                                        study_oid,
                                        version_oid,
                                        event_oid,
                                    ),
                                ),
                                "StartDate": event["StartDate"],
                                "EventStatus": event["EventStatus"],
                                "EventWorkflowStatus": event["EventWorkflowStatus"],
                                "FormOID": form_oid,
                                "FormRepeatKey": projected_form["FormRepeatKey"],
                                "FormName": _source_or_metadata(
                                    projected_form["FormName"],
                                    _metadata_name(
                                        names,
                                        form_kind,
                                        study_oid,
                                        version_oid,
                                        form_oid,
                                    ),
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
                                    item_group["ItemGroupName"],
                                    _metadata_name(
                                        names,
                                        "group",
                                        study_oid,
                                        version_oid,
                                        group_oid,
                                    ),
                                ),
                                "TransactionType": item_group["TransactionType"],
                                "ItemOID": item_oid,
                                "ItemName": _source_or_metadata(
                                    _extension_attribute(element, "ItemName"),
                                    _metadata_name(
                                        names,
                                        "item",
                                        study_oid,
                                        version_oid,
                                        item_oid,
                                    ),
                                ),
                                "Value": value,
                                "ValuePresent": value_present,
                                "IsNull": is_null,
                                "SourceOrdinal": source_ordinal,
                            }
                        )
                    except ValidationError as error:
                        raise ODMError(
                            f"invalid item at source ordinal {source_ordinal}: {error}"
                        ) from error
                    yield row
                    _release(element)
                elif (
                    element.getparent() is not None
                    and _local_name(element.getparent().tag) == "ItemData"
                ):
                    pass
                elif name == "ItemGroupData":
                    if not groups:
                        raise ODMError("unbalanced ItemGroupData")
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
        raise ODMError(f"malformed ODM XML: {error}") from error

    if odm_namespace is None:
        raise ODMError("document has no ODM root")


def read_odm(
    source: str | Path,
    *,
    archive_member: str | None = None,
    max_archive_members: int = 10_000,
    max_expanded_bytes: int = 256 * 1024 * 1024,
    max_xml_depth: int = 64,
) -> pl.DataFrame:
    """Read all clinical items into one fixed-schema Polars DataFrame."""
    return rows_to_frame(
        iter_odm_records(
            source,
            archive_member=archive_member,
            max_archive_members=max_archive_members,
            max_expanded_bytes=max_expanded_bytes,
            max_xml_depth=max_xml_depth,
        )
    )
