"""Deterministic Define-XML 2.1 generation and offline schema validation."""

from __future__ import annotations

from dataclasses import dataclass, field
from functools import cache
from pathlib import Path
from typing import Any

from lxml import etree

from yamaa.models import ValueResult, convert_value
from yamaa.specification import SpecificationError, ValidationDiagnostic
from yamaa.specification.models import DocumentReferenceClass
from yamaa.submission.composition import ComposedStudy, sas_name


def scalar_text(value: Any) -> str:
    if isinstance(value, float):
        converted = convert_value(value, "str")
        if not isinstance(converted, ValueResult) or not isinstance(
            converted.value, str
        ):
            raise TypeError("a codelist number must be finite")
        return converted.value
    return str(value)


@dataclass
class Element:
    name: str
    attributes: dict[str, Any] = field(default_factory=dict)
    children: list[Element] = field(default_factory=list)
    text: str | None = None
    path: str = "output.path"
    attribute_paths: dict[str, str] = field(default_factory=dict)

    def add(
        self,
        name: str,
        attributes: dict[str, Any] | None = None,
        *,
        text: str | None = None,
        path: str | None = None,
    ) -> Element:
        child = Element(name, attributes or {}, text=text, path=path or self.path)
        self.children.append(child)
        return child


def _escape(value: Any, path: str, *, attribute: bool = False) -> str:
    text = scalar_text(value)
    if "\r" in text or "\t" in text or (attribute and "\n" in text):
        raise SpecificationError(
            [
                ValidationDiagnostic(
                    condition="untransportable_text",
                    spec_paths=(path,),
                    requirement="REQ-1025",
                    context={"value": text},
                )
            ]
        )
    escaped = text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    return escaped.replace('"', "&quot;") if attribute else escaped


def _lines(element: Element, level: int = 0) -> list[str]:
    indent = "  " * level
    attributes = "".join(
        f' {name}="{_escape(value, element.attribute_paths.get(name, element.path), attribute=True)}"'
        for name, value in element.attributes.items()
        if value is not None
    )
    opening = f"{indent}<{element.name}{attributes}"
    if element.text is not None:
        return [f"{opening}>{_escape(element.text, element.path)}</{element.name}>"]
    if not element.children:
        return [f"{opening}/>"]
    lines = [opening + ">"]
    for child in element.children:
        lines.extend(_lines(child, level + 1))
    lines.append(f"{indent}</{element.name}>")
    return lines


@cache
def _schema() -> etree.XMLSchema:
    path = Path(__file__).parent / "schemas" / "define" / "2.1" / "define2-1-0.xsd"
    return etree.XMLSchema(
        etree.parse(str(path), etree.XMLParser(no_network=True, resolve_entities=False))
    )


def validate_define_xml(content: bytes) -> None:
    """Reject schema-invalid XML before publication (REQ-1026)."""
    try:
        root = etree.fromstring(
            content, etree.XMLParser(no_network=True, resolve_entities=False)
        )
        _schema().assertValid(root)
    except (etree.XMLSyntaxError, etree.DocumentInvalid) as error:
        raise SpecificationError(
            [
                ValidationDiagnostic(
                    condition="value_not_permitted",
                    spec_paths=("output.path",),
                    requirement="REQ-1026",
                    context={"schema_violation": str(error)},
                )
            ]
        ) from error


def render_define_xml(study: ComposedStudy) -> bytes:
    """Render exactly the bytes REQ-1005..1008 fix, then validate them."""
    doc = study.document
    language = doc["language"]

    def translated(
        parent: Element, name: str, text: str, *, path: str | None = None
    ) -> None:
        translated_text = parent.add(name, path=path).add(
            "TranslatedText", {"xml:lang": language}, text=text
        )
        translated_text.attribute_paths["xml:lang"] = "language"

    def references(parent: Element, refs: Any) -> None:
        for reference in refs or []:
            identifier = reference if isinstance(reference, str) else reference.document
            child = parent.add("def:DocumentRef", {"leafID": f"LF.{identifier}"})
            if (
                isinstance(reference, DocumentReferenceClass)
                and reference.pages is not None
            ):
                pages = reference.pages
                child.add(
                    "def:PDFPageRef",
                    {"PageRefs": pages.refs, "Type": pages.type, "Title": pages.title},
                )

    def leaf(
        parent: Element,
        identifier: str,
        href: str,
        title: str,
        *,
        path: str | None = None,
    ) -> None:
        parent.add(
            "def:leaf", {"ID": f"LF.{identifier}", "xlink:href": href}, path=path
        ).add("def:title", text=title, path=path + ".title" if path else None)

    odm = Element(
        "ODM",
        {
            "xmlns": "http://www.cdisc.org/ns/odm/v1.3",
            "xmlns:def": "http://www.cdisc.org/ns/def/v2.1",
            "xmlns:xlink": "http://www.w3.org/1999/xlink",
            "ODMVersion": "1.3.2",
            "FileType": "Snapshot",
            "FileOID": doc["file_oid"],
            "CreationDateTime": doc["creation_datetime"],
            "Originator": doc.get("originator"),
            "SourceSystem": doc.get("source_system"),
            "SourceSystemVersion": doc.get("source_system_version"),
            "def:Context": doc["context"],
        },
        path="study",
    )
    odm.attribute_paths = {
        "FileOID": "file_oid",
        "CreationDateTime": "creation_datetime",
        "Originator": "originator",
        "SourceSystem": "source_system",
        "SourceSystemVersion": "source_system_version",
        "def:Context": "context",
    }
    root = odm.add("Study", {"OID": f"STDY.{doc['study']['id']}"})
    globals_ = root.add("GlobalVariables")
    for name, key in (
        ("StudyName", "name"),
        ("StudyDescription", "description"),
        ("ProtocolName", "protocol_name"),
    ):
        globals_.add(name, text=doc["study"][key], path=f"study.{key}")
    version = doc["metadata_version"]
    metadata = root.add(
        "MetaDataVersion",
        {
            "OID": f"MDV.{version['id']}",
            "Name": version["name"],
            "Description": version.get("description"),
            "def:DefineVersion": doc["define_version"],
        },
        path="metadata_version",
    )
    standards = metadata.add("def:Standards", path="standards")
    for standard in doc["standards"]:
        standards.add(
            "def:Standard",
            {
                "OID": f"STD.{standard['id']}",
                "Name": standard["name"],
                "Type": standard["type"],
                "PublishingSet": standard.get("publishing_set"),
                "Version": standard["version"],
                "Status": standard["status"],
            },
            path=f"standards.{standard['id']}",
        )
    for kind, name in (
        ("annotated_crf", "def:AnnotatedCRF"),
        ("supplemental", "def:SupplementalDoc"),
    ):
        matching = [
            entry for entry in doc.get("documents") or [] if entry["kind"] == kind
        ]
        if matching:
            parent = metadata.add(name, path="documents")
            references(parent, [entry["id"] for entry in matching])
    for dataset in study.datasets:
        spec = dataset.specification
        submission = spec.submission
        assert submission is not None
        group = metadata.add(
            "ItemGroupDef",
            {
                "OID": f"IG.{dataset.id}",
                "Name": dataset.id,
                "Domain": (submission.domain or spec.domain)
                if dataset.family != "adam"
                else None,
                "Purpose": "Analysis" if dataset.family == "adam" else "Tabulation",
                "SASDatasetName": dataset.id if sas_name(dataset.id) else None,
                "Repeating": "Yes" if submission.repeating else "No",
                "IsReferenceData": "Yes" if submission.reference_data else "No",
                "def:Structure": submission.structure,
                "def:ArchiveLocationID": f"LF.{dataset.id}",
                "def:StandardOID": f"STD.{dataset.standard}",
                "def:CommentOID": f"COM.{dataset.id}"
                if submission.comment is not None
                else None,
                "def:HasNoData": "Yes" if dataset.entry.get("has_no_data") else None,
            },
            path=f"datasets.{dataset.id}.submission",
        )
        translated(group, "Description", submission.label)
        for index, column in enumerate(dataset.columns, 1):
            column_meta = column.submission
            assert column_meta is not None
            group.add(
                "ItemRef",
                {
                    "ItemOID": f"IT.{dataset.id}.{column.name}",
                    "OrderNumber": index,
                    "Mandatory": "Yes" if column_meta.mandatory else "No",
                    "KeySequence": spec.keys.index(column.name) + 1
                    if column.name in spec.keys
                    else None,
                    "Role": column_meta.role,
                    "MethodOID": f"MT.{dataset.id}.{column.name}"
                    if column_meta.method is not None
                    else None,
                },
            )
        class_ = group.add("def:Class", {"Name": submission.class_name})
        if submission.subclass:
            class_.add("def:SubClass", {"Name": submission.subclass})
        leaf(group, dataset.id, dataset.href, Path(dataset.href).name)
    for dataset in study.datasets:
        for column in dataset.columns:
            col = column.submission
            assert (
                col is not None and col.origin is not None and column.label is not None
            )
            item = metadata.add(
                "ItemDef",
                {
                    "OID": f"IT.{dataset.id}.{column.name}",
                    "Name": column.name,
                    "DataType": col.data_type,
                    "Length": col.length,
                    "SignificantDigits": col.significant_digits,
                    "SASFieldName": column.name if sas_name(column.name) else None,
                    "def:DisplayFormat": col.display_format,
                    "def:CommentOID": f"COM.{dataset.id}.{column.name}"
                    if col.comment is not None
                    else None,
                },
                path=f"datasets.{dataset.id}.columns.{column.name}.submission",
            )
            translated(
                item,
                "Description",
                column.label,
                path=f"datasets.{dataset.id}.columns.{column.name}.label",
            )
            if col.codelist:
                item.add("CodeListRef", {"CodeListOID": f"CL.{col.codelist}"})
            origin = item.add(
                "def:Origin",
                {"Type": col.origin.type, "Source": col.origin.source},
                path=item.path + ".origin",
            )
            if col.origin.description:
                translated(origin, "Description", col.origin.description)
            references(origin, col.origin.documents)
    for codelist in doc.get("codelists") or []:
        standard = codelist.get("standard")
        code = metadata.add(
            "CodeList",
            {
                "OID": f"CL.{codelist['id']}",
                "Name": codelist["name"],
                "DataType": codelist["data_type"],
                "def:StandardOID": f"STD.{standard}" if standard else None,
                "def:IsNonStandard": "Yes"
                if standard is None and codelist.get("items") is not None
                else None,
                "SASFormatName": codelist.get("format_name"),
            },
            path=f"codelists.{codelist['id']}",
        )
        for index, item in enumerate(codelist.get("items") or []):
            node = code.add(
                "CodeListItem" if item.get("decode") is not None else "EnumeratedItem",
                {
                    "CodedValue": item["value"],
                    "Rank": item.get("rank"),
                    "def:ExtendedValue": "Yes" if item.get("extended") else None,
                },
                path=f"codelists.{codelist['id']}.items[{index}]",
            )
            if item.get("decode") is not None:
                translated(node, "Decode", item["decode"])
            if item.get("alias"):
                node.add("Alias", {"Context": "nci:ExtCodeID", "Name": item["alias"]})
        external = codelist.get("external")
        if external is not None:
            code.add(
                "ExternalCodeList",
                {
                    "Dictionary": external["dictionary"],
                    "Version": external["version"],
                    "href": external.get("href"),
                },
            )
        if codelist.get("alias"):
            code.add("Alias", {"Context": "nci:ExtCodeID", "Name": codelist["alias"]})
    for dataset in study.datasets:
        for column in dataset.columns:
            assert column.submission is not None
            method = column.submission.method
            if method is None:
                continue
            name = f"{dataset.id}.{column.name}"
            node = metadata.add(
                "MethodDef",
                {
                    "OID": f"MT.{name}",
                    "Name": name if isinstance(method, str) else method.name or name,
                    "Type": "Computation" if isinstance(method, str) else method.type,
                },
                path=f"datasets.{dataset.id}.columns.{column.name}.submission.method",
            )
            translated(
                node,
                "Description",
                method if isinstance(method, str) else method.description,
            )
            if not isinstance(method, str):
                if method.expression:
                    node.add(
                        "FormalExpression",
                        {"Context": method.expression.context},
                        text=method.expression.code,
                        path=node.path + ".expression.code",
                    )
                references(node, method.documents)
    for dataset in study.datasets:
        assert dataset.specification.submission is not None
        comments = [
            (
                dataset.id,
                dataset.specification.submission.comment,
                f"datasets.{dataset.id}.submission.comment",
            )
        ]
        comments.extend(
            (
                f"{dataset.id}.{column.name}",
                column.submission.comment,
                f"datasets.{dataset.id}.columns.{column.name}.submission.comment",
            )
            for column in dataset.columns
            if column.submission is not None
        )
        for identifier, comment, path in comments:
            if comment is None:
                continue
            node = metadata.add(
                "def:CommentDef",
                {"OID": f"COM.{identifier}"},
                path=path,
            )
            translated(
                node,
                "Description",
                comment if isinstance(comment, str) else comment.text,
            )
            if not isinstance(comment, str):
                references(node, comment.documents)
    for document in doc.get("documents") or []:
        leaf(
            metadata,
            document["id"],
            document["href"],
            document["title"],
            path=f"documents.{document['id']}",
        )
    lines = ['<?xml version="1.0" encoding="UTF-8"?>']
    if doc.get("stylesheet"):
        href = _escape(doc["stylesheet"], "stylesheet", attribute=True)
        lines.append(f'<?xml-stylesheet type="text/xsl" href="{href}"?>')
    content = ("\n".join([*lines, *_lines(odm)]) + "\n").encode("utf-8")
    validate_define_xml(content)
    return content
