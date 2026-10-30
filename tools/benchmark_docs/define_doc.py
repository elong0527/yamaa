"""Render authored Define-XML metadata as resolved tables for human review."""

import html
import xml.etree.ElementTree as ET

ODM = "http://www.cdisc.org/ns/odm/v1.3"
DEFINE = ("http://www.cdisc.org/ns/def/v2.0", "http://www.cdisc.org/ns/def/v2.1")
COMPARATORS = {
    "EQ": "=",
    "NE": "!=",
    "LT": "<",
    "LE": "<=",
    "GT": ">",
    "GE": ">=",
    "IN": "in",
    "NOTIN": "not in",
}
HEADERS = (
    "Variable",
    "Label",
    "Type",
    "Length",
    "Significant digits",
    "Required",
    "Role / key",
    "Codelist / values",
    "Origin",
    "Method / comment",
)


def attribute(element, name):
    return next(
        (value for key, value in element.attrib.items() if key.split("}")[-1] == name),
        "",
    )


def description(element):
    return " / ".join(
        "".join(text.itertext())
        for text in element.findall("{*}Description/{*}TranslatedText")
    )


def table(title, headers, rows):
    return (
        '<div class="define-scroll" tabindex="0" role="region" '
        f'aria-label="{html.escape(title, quote=True)}">'
        '<table class="define-table">'
        f"<caption>{html.escape(title)}</caption><thead><tr>"
        + "".join(f'<th scope="col">{html.escape(cell)}</th>' for cell in headers)
        + "</tr></thead><tbody>"
        + "".join(
            "<tr>" + "".join(f"<td>{html.escape(cell)}</td>" for cell in row) + "</tr>"
            for row in rows
        )
        + "</tbody></table></div>"
    )


def documents(element, definitions):
    result = []
    for reference in element.findall(".//{*}DocumentRef"):
        leaf_id = reference.get("leafID", "")
        leaf = definitions.get(leaf_id)
        title = (
            leaf_id
            if leaf is None
            else (leaf.findtext("{*}title", "") + " (" + attribute(leaf, "href") + ")")
        )
        pages = [
            attribute(page, "PageRefs")
            or "-".join(filter(None, (page.get("FirstPage"), page.get("LastPage"))))
            for page in reference.findall("{*}PDFPageRef")
        ]
        result.append(title + ("; pages " + ", ".join(pages) if pages else ""))
    return result


def codelist(item, definitions):
    result = []
    for reference in item.findall("{*}CodeListRef"):
        oid = reference.get("CodeListOID", "")
        definition = definitions.get(oid)
        if definition is None:
            result.append(oid)
            continue
        values = []
        for entry in definition:
            if "CodedValue" in entry.attrib:
                decoded = " / ".join(
                    text.text or ""
                    for text in entry.findall("{*}Decode/{*}TranslatedText")
                )
                values.append(
                    entry.get("CodedValue") + (" = " + decoded if decoded else "")
                )
            elif entry.tag.endswith("}ExternalCodeList"):
                values.append(
                    " ".join(
                        filter(
                            None,
                            (
                                entry.get("Dictionary"),
                                entry.get("Version"),
                            ),
                        )
                    )
                )
        result.append(
            definition.get("Name", oid) + " (" + oid + ")\n" + "\n".join(values)
        )
    return "\n".join(result)


def condition(reference, definitions):
    clauses = []
    for link in reference.findall("{*}WhereClauseRef"):
        oid = link.get("WhereClauseOID", "")
        clause = definitions.get(oid)
        if clause is None:
            clauses.append(oid)
            continue
        for check in clause.findall("{*}RangeCheck"):
            item_oid = attribute(check, "ItemOID")
            item = definitions.get(item_oid)
            name = item.get("Name", item_oid) if item is not None else item_oid
            comparator = check.get("Comparator", "")
            values = ", ".join(
                value.text or "" for value in check.findall("{*}CheckValue")
            )
            clauses.append(f"{name} {COMPARATORS.get(comparator, comparator)} {values}")
    return " AND ".join(clauses)


def referenced_text(oid, definitions):
    if not oid:
        return ""
    definition = definitions.get(oid)
    if definition is None:
        return oid
    return "\n".join(
        filter(
            None,
            (
                description(definition),
                *(value.text for value in definition.findall("{*}FormalExpression")),
                *documents(definition, definitions),
            ),
        )
    )


def item_row(reference, definitions):
    oid = reference.get("ItemOID", "")
    item = definitions.get(oid)
    if item is None:
        return [oid] + [""] * (len(HEADERS) - 1)
    origins = []
    for origin in item.findall("{*}Origin"):
        origins.append(
            "; ".join(
                filter(
                    None,
                    (
                        origin.get("Type"),
                        origin.get("Source"),
                        description(origin),
                        *documents(origin, definitions),
                    ),
                )
            )
        )
    methods = []
    for method_oid in (reference.get("MethodOID"), attribute(item, "CommentOID")):
        if not method_oid:
            continue
        methods.append(referenced_text(method_oid, definitions))
    role = reference.get("Role", "")
    if reference.get("KeySequence"):
        role += ("; " if role else "") + "Key " + reference.get("KeySequence")
    return [
        item.get("Name", oid),
        description(item),
        item.get("DataType", ""),
        item.get("Length", ""),
        item.get("SignificantDigits", ""),
        reference.get("Mandatory", ""),
        role,
        codelist(item, definitions),
        "\n".join(origins),
        "\n".join(methods),
    ]


def render_metadata(metadata, prefix):
    definitions = {
        element.get("OID") or element.get("ID"): element
        for element in metadata.iter()
        if element.get("OID") or element.get("ID")
    }
    parts, navigation = [], []
    for index, group in enumerate(metadata.findall("{*}ItemGroupDef")):
        name = group.get("Name", group.get("OID", "Dataset"))
        anchor = f"{prefix}-dataset-{index}"
        navigation.append(f'<a href="#{anchor}">{html.escape(name)}</a>')
        parts.append(f'<section id="{anchor}"><h4>{html.escape(name)}</h4>')
        facts = [
            description(group),
            group.get("Purpose"),
            attribute(group, "Structure"),
        ]
        class_node = group.find("{*}Class")
        facts.append(
            class_node.get("Name")
            if class_node is not None
            else attribute(group, "Class")
        )
        facts.append(referenced_text(attribute(group, "CommentOID"), definitions))
        parts.append("<p>" + html.escape("; ".join(filter(None, facts))) + "</p>")
        references = group.findall("{*}ItemRef")
        parts.append(
            table(
                name + " variables",
                HEADERS,
                [item_row(ref, definitions) for ref in references],
            )
        )
        for reference in references:
            item = definitions.get(reference.get("ItemOID"))
            if item is None:
                continue
            for link in item.findall("{*}ValueListRef"):
                values = definitions.get(link.get("ValueListOID"))
                if values is None:
                    continue
                rows = [
                    [condition(ref, definitions)] + item_row(ref, definitions)
                    for ref in values.findall("{*}ItemRef")
                ]
                parts.append(
                    table(
                        name + "." + item.get("Name", "") + " value-level metadata",
                        ("Condition", *HEADERS),
                        rows,
                    )
                )
        parts.append("</section>")
    return (
        '<nav class="define-nav" aria-label="Define-XML datasets">'
        + " ".join(navigation)
        + "</nav>"
        + "".join(parts)
    )


def render_define(text, prefix):
    """Return a readable Define-XML view, or None for other/malformed XML.

    Read the authored XML only: no stylesheet, external entity or engine runs.
    The caller keeps the original source available, including metadata outside
    the resolved variable and value-level tables.
    """
    try:
        root = ET.fromstring(text)
    except ET.ParseError:
        return None
    if root.tag != f"{{{ODM}}}ODM":
        return None
    parts = []
    for study_index, study in enumerate(root.findall(f"{{{ODM}}}Study")):
        for index, metadata in enumerate(study.findall(f"{{{ODM}}}MetaDataVersion")):
            version = next(
                (
                    metadata.get(f"{{{ns}}}DefineVersion")
                    for ns in DEFINE
                    if metadata.get(f"{{{ns}}}DefineVersion")
                ),
                None,
            )
            if not version:
                continue
            name = study.findtext("{*}GlobalVariables/{*}StudyName", "")
            parts.append(
                f"<h3>Define-XML {html.escape(version)}: {html.escape(name)}</h3>"
            )
            summary = [
                study.findtext("{*}GlobalVariables/{*}StudyDescription", ""),
                study.findtext("{*}GlobalVariables/{*}ProtocolName", ""),
                " ".join(
                    filter(
                        None,
                        (
                            attribute(metadata, "StandardName"),
                            attribute(metadata, "StandardVersion"),
                        ),
                    )
                ),
            ]
            summary.extend(
                " ".join(filter(None, (standard.get("Name"), standard.get("Version"))))
                for standard in metadata.findall("{*}Standards/{*}Standard")
            )
            parts.append("<p>" + html.escape("; ".join(filter(None, summary))) + "</p>")
            parts.append(render_metadata(metadata, f"{prefix}-{study_index}-{index}"))
    return (
        '<div class="define-document">' + "".join(parts) + "</div>" if parts else None
    )
