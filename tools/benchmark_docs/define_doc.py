"""Render authored Define-XML metadata as resolved tables for human review.

The tables take the columns of the CDISC Define-XML 2.1 stylesheet
(define2-1.xsl), so a reviewer reads the document as a submission's define.xml
reads in a browser: the datasets with their keys, then each dataset's
variables. As there, a Role column appears only for a dataset whose variables
declare roles.
"""

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
DATASET_HEADERS = (
    "Dataset",
    "Description",
    "Class",
    "Structure",
    "Purpose",
    "Keys",
    "Documentation",
    "Location",
)
VARIABLE_HEADERS = (
    "Variable",
    "Label / Description",
    "Type",
    "Length or Display Format",
    "Controlled Terms or ISO Format",
    "Origin / Source / Method / Comment",
)
ROLE_HEADER = "Role"
ROLE_AT = VARIABLE_HEADERS.index("Type") + 1
# The data types whose values the stylesheet labels ISO 8601 when no codelist
# constrains them.
ISO_8601 = {
    "date",
    "time",
    "datetime",
    "partialDate",
    "partialTime",
    "partialDatetime",
    "incompleteDatetime",
    "durationDatetime",
}


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


def controlled_terms(item, definitions):
    return codelist(item, definitions) or (
        "ISO 8601" if item.get("DataType") in ISO_8601 else ""
    )


def item_row(reference, definitions, roles):
    oid = reference.get("ItemOID", "")
    item = definitions.get(oid)
    width = len(VARIABLE_HEADERS) + roles
    if item is None:
        return [oid] + [""] * (width - 1)
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
    for method_oid in (reference.get("MethodOID"), attribute(item, "CommentOID")):
        if not method_oid:
            continue
        origins.append(referenced_text(method_oid, definitions))
    row = [
        item.get("Name", oid),
        description(item),
        item.get("DataType", ""),
        attribute(item, "DisplayFormat") or item.get("Length", ""),
        controlled_terms(item, definitions),
        "\n".join(origins),
    ]
    if roles:
        row.insert(ROLE_AT, reference.get("Role", ""))
    return row


def variable_headers(roles):
    headers = list(VARIABLE_HEADERS)
    if roles:
        headers.insert(ROLE_AT, ROLE_HEADER)
    return headers


def dataset_row(group, definitions):
    class_node = group.find("{*}Class")
    if class_node is None:
        named_class = attribute(group, "Class")
    else:
        named_class = " - ".join(
            filter(
                None,
                (
                    class_node.get("Name"),
                    *(sub.get("Name") for sub in class_node.findall("{*}SubClass")),
                ),
            )
        )
    keys = []
    for reference in group.findall("{*}ItemRef"):
        sequence = reference.get("KeySequence")
        if sequence is None:
            continue
        try:
            position = int(sequence)
        except ValueError:
            return None
        if position < 1:
            return None
        keys.append((position, reference.get("ItemOID", "")))
    keys.sort()
    names = []
    for _, oid in keys:
        item = definitions.get(oid)
        names.append(oid if item is None else item.get("Name", oid))
    locations = [
        leaf.findtext("{*}title", "") or attribute(leaf, "href")
        for leaf in group.findall("{*}leaf")
    ]
    return [
        group.get("Name", group.get("OID", "")),
        description(group),
        named_class,
        attribute(group, "Structure"),
        group.get("Purpose", ""),
        ", ".join(names),
        referenced_text(attribute(group, "CommentOID"), definitions),
        "\n".join(locations),
    ]


def render_metadata(metadata, prefix):
    definitions = {
        element.get("OID") or element.get("ID"): element
        for element in metadata.iter()
        if element.get("OID") or element.get("ID")
    }
    groups = metadata.findall("{*}ItemGroupDef")
    datasets = [dataset_row(group, definitions) for group in groups]
    if any(row is None for row in datasets):
        return None
    parts = [
        table(
            "Datasets",
            DATASET_HEADERS,
            datasets,
        )
    ]
    navigation = []
    for index, group in enumerate(groups):
        name = group.get("Name", group.get("OID", "Dataset"))
        anchor = f"{prefix}-dataset-{index}"
        navigation.append(f'<a href="#{anchor}">{html.escape(name)}</a>')
        parts.append(f'<section id="{anchor}"><h4>{html.escape(name)}</h4>')
        references = group.findall("{*}ItemRef")
        roles = any(reference.get("Role") for reference in references)
        parts.append(
            table(
                name + " variables",
                variable_headers(roles),
                [item_row(ref, definitions, roles) for ref in references],
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
                value_references = values.findall("{*}ItemRef")
                value_roles = any(ref.get("Role") for ref in value_references)
                rows = [
                    [condition(ref, definitions)]
                    + item_row(ref, definitions, value_roles)
                    for ref in value_references
                ]
                parts.append(
                    table(
                        name + "." + item.get("Name", "") + " value-level metadata",
                        ("Where Condition", *variable_headers(value_roles)),
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
            rendered = render_metadata(metadata, f"{prefix}-{study_index}-{index}")
            if rendered is None:
                return None
            parts.append(rendered)
    return (
        '<div class="define-document">' + "".join(parts) + "</div>" if parts else None
    )
