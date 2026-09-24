#!/usr/bin/env python3
"""Render the human-review "Mapping spec" section of a benchmark dashboard.

The section is generated from the benchmark's spec.yaml as a familiar Excel
specification: one row per output column. The header layout follows the
sponsor's SDTM and ADaM variable sheets, chosen by the dataset standard, so a
reviewer reads the derived spec in the same shape they author it. Rendering is
deterministic: an unchanged spec produces identical HTML.

Values come from each column's `submission:` block when present -- `core`,
`length`, `codelist`, `origin.type`, `method`, `comment` -- and fall back to
the derivation itself: a plain-language conversion rule and a Define-XML 2.1
origin classification (Assigned, Collected, Derived, Not Available, Other,
Predecessor, Protocol).

This file is ASCII-only to satisfy the repository source lint; arrows and
other non-ASCII glyphs are emitted as HTML character references.
"""

import html
from pathlib import Path

# ASCII escape sequences keep this file ASCII-clean for the repository source
# lint; the dashboard's final encode("ascii", "xmlcharrefreplace") turns them
# into HTML character references.
ARROW = "\u2192"
MAPPING_TABS_JS_PATH = Path(__file__).resolve().parent / "mapping-tabs.js"

# Sentence templates: one plain-language mapping rule per derivation shape.


def describe_literal(value):
    return 'Set constant value "' + str(value) + '".'


def describe_source(src):
    filt = src.get("filter")
    var = src.get("variable")
    if filt:
        return "Copy value from " + str(var) + " where " + str(filt) + "."
    return "Copy value from " + str(var) + "."


def describe_mapping(mapping):
    src = mapping.get("source", {})
    if isinstance(src, str):
        var, filt = src, None
    else:
        var = src.get("variable")
        filt = src.get("filter")
    where = " where " + str(filt) if filt else ""
    pairs = ", ".join(
        '"' + str(k) + '" ' + ARROW + ' "' + str(v) + '"'
        for k, v in mapping.get("dict", {}).items()
    )
    strict = mapping.get("strict", False)
    absent = describe_mapping_handler(mapping, "missing", strict)
    if "unmapped" in mapping or strict:
        unlisted = describe_mapping_handler(mapping, "unmapped", strict)
    else:
        # Without `unmapped`, `missing` answers the unlisted value too.
        unlisted = absent
    if absent == unlisted:
        tail = "" if absent is None else "; missing or unlisted values " + absent
    else:
        tail = (
            "; missing values "
            + (absent or "stay missing")
            + "; unlisted values "
            + (unlisted or "stay missing")
        )
    return "Recode " + str(var) + where + ": " + pairs + tail + "."


def describe_mapping_handler(mapping, handler, strict):
    """What one mapping event yields: a literal, an error, or None for missing."""
    if handler in mapping:
        value = mapping[handler]
        return None if value is None else ARROW + ' "' + str(value) + '"'
    return "are errors" if strict else None


def describe_case(branches):
    parts = []
    has_otherwise = False
    for branch in branches:
        when = branch.get("when")
        then = branch.get("then", {})
        then_text = then.get("literal", then) if isinstance(then, dict) else then
        if when:
            parts.append("If " + str(when) + ' then "' + str(then_text) + '"')
        else:
            has_otherwise = True
            parts.append('otherwise "' + str(then_text) + '"')
    if not has_otherwise:
        parts.append("otherwise blank")
    return "; ".join(parts) + "."


def describe_row_number(node):
    window = node.get("window", {})
    if isinstance(window, str):
        return "Row number using window " + window + "."
    groups = ", ".join(str(g) for g in window.get("group_by", []))
    order = ", ".join(str(o) for o in window.get("order_by", []))
    text = "Row number within each (" + groups + ")"
    if order:
        text += ", ordered by " + order
    return text + "."


def describe_compute(node):
    return "Compute " + str(node.get("expr")) + "."


def describe_baseline_flag(node):
    window = node.get("window", {})
    scope = (
        " using window " + window
        if isinstance(window, str)
        else " within each ("
        + ", ".join(str(g) for g in window.get("group_by", []))
        + ")"
    )
    return (
        '"Y" for the last record with '
        + str(node.get("date"))
        + " on or before "
        + str(node.get("reference_date"))
        + scope
        + "; blank otherwise."
    )


def describe_date_impute(node):
    bits = []
    if node.get("month") is not None:
        bits.append("month=" + str(node["month"]))
    if node.get("day") is not None:
        bits.append("day=" + str(node["day"]))
    return (
        "Impute incomplete date from "
        + str(node.get("source"))
        + ", defaulting "
        + ", ".join(bits)
        + "; missing or invalid input yields null."
    )


def describe_derivation(derivation):
    """Plain-language mapping rule for one column-level derivation."""
    if isinstance(derivation, str):
        return "Copy value from " + derivation + "."
    if not isinstance(derivation, dict):
        return str(derivation)
    if "literal" in derivation:
        return describe_literal(derivation["literal"])
    source = derivation.get("source")
    if isinstance(source, dict):
        return describe_source(source)
    if "mapping" in derivation:
        return describe_mapping(derivation["mapping"])
    if "case" in derivation:
        return describe_case(derivation["case"])
    if "row_number" in derivation:
        return describe_row_number(derivation["row_number"])
    if "compute" in derivation:
        return describe_compute(derivation["compute"])
    if "baseline_flag" in derivation:
        return describe_baseline_flag(derivation["baseline_flag"])
    if "date_impute" in derivation:
        return describe_date_impute(derivation["date_impute"])
    return str(derivation)


def classify_origin(derivation, input_names):
    """Origin per the Define-XML 2.1 vocabulary."""
    if isinstance(derivation, str):
        base = derivation.split(".")[0]
        return "Collected" if base in input_names else "Derived"
    if not isinstance(derivation, dict):
        return "Derived"
    if "literal" in derivation:
        return "Assigned"
    if "source" in derivation or "mapping" in derivation:
        return "Collected"
    return "Derived"


def submission(col):
    """The column's `submission:` block, or an empty mapping."""
    block = col.get("submission")
    return block if isinstance(block, dict) else {}


# Every ADaM dataset name starts with "AD", but so could a future SDTM
# custom domain -- the prefix alone is not a classifier. Keep an explicit
# registry of the ADaM datasets the corpus uses; an AD-prefixed name that
# is not registered fails loudly instead of silently picking the wrong
# sheet headers.
ADAM_DATASETS = frozenset(
    {
        "ADAE",
        "ADCE",
        "ADCM",
        "ADEG",
        "ADEX",
        "ADLB",
        "ADLBC",
        "ADOE",
        "ADQS",
        "ADRS",
        "ADSL",
        "ADTR",
        "ADTTE",
        "ADVS",
    }
)


def is_adam(spec):
    """True when the spec's domain is a registered ADaM dataset.

    Classification is an exact lookup, not the "AD" prefix: a future SDTM
    custom domain starting with "AD" (or a new ADaM dataset) must not
    silently render the wrong sheet headers. An AD-prefixed name outside
    the registry raises, so the new domain gets registered -- or the
    domain fixed -- explicitly.
    """
    domain = str(spec.get("domain") or "").upper()
    if domain in ADAM_DATASETS:
        return True
    if domain.startswith("AD"):
        raise ValueError(
            f"domain {domain!r} starts with 'AD' but is not a registered ADaM "
            "dataset; add it to ADAM_DATASETS in mapping_doc.py or fix the domain"
        )
    return False


def type_label(col_type, adam):
    """Standard-specific rendering of a yamaa type.

    SDTM writes Char / Num; ADaM writes character / numeric. SDTM dates are
    ISO 8601 character values; ADaM dates are numeric.
    """
    text = str(col_type or "")
    numeric = text in ("int", "float")
    date = text in ("date", "datetime")
    if adam:
        if numeric or date:
            return "numeric"
        if text == "str":
            return "character"
        return text
    if numeric:
        return "Num"
    if text == "str" or date:
        return "Char"
    return text


def submission_origin(col):
    origin = submission(col).get("origin")
    if isinstance(origin, dict):
        return str(origin.get("type") or "")
    if isinstance(origin, str):
        return origin
    return ""


def controlled_terms(col, code_by_var):
    """Codelist or format binding: the declared `submission.codelist`, else the
    permitted values enforced by an `allowed_values` verification."""
    code = submission(col).get("codelist")
    if code:
        return str(code)
    return code_by_var.get(col.get("name", ""), "")


def conversion_definition(col):
    """The authored `submission.method`, else the derivation in plain language."""
    method = submission(col).get("method")
    if method:
        return str(method)
    return describe_derivation(col.get("derivation"))


def define_comment(col):
    comment = submission(col).get("comment")
    if isinstance(comment, dict):
        return str(comment.get("text") or "")
    return str(comment) if comment else ""


def codelists(columns):
    out = []
    for col in columns:
        for check in col.get("verifications", []) or []:
            if isinstance(check, dict) and "allowed_values" in check:
                values = check["allowed_values"].get("values", [])
                out.append((col["name"], ", ".join(str(v) for v in values)))
    return out


def describe_row_template_derivation(derivation):
    if isinstance(derivation, str):
        return "Copy " + derivation
    if isinstance(derivation, dict):
        if "literal" in derivation:
            return 'Constant "' + str(derivation["literal"]) + '"'
        if "source" in derivation:
            return "Copy " + str(derivation["source"])
        compute = derivation.get("compute", {})
        if isinstance(compute, dict) and compute.get("expr"):
            return "Compute " + str(compute["expr"])
    return str(derivation)


# Variable-sheet header layouts, following the sponsor's Excel specifications.
SDTM_HEADERS = [
    "Variable Name",
    "Variable Label",
    "Type",
    "Length",
    "Controlled Terms or Format",
    "Origin",
    "Core",
    "Conversion Definition",
    "Variable Type",
    "Variable Order",
    "Comments for Define",
]
ADAM_HEADERS = [
    "Dataset Name",
    "Variable Name",
    "Variable Label",
    "Type",
    "Codelist/Controlled Terms",
    "Core",
    "Computational Method",
    "Origin",
]


def sdtm_variable_type(spec):
    """The Variable Type cell distinguishes a parent SDTM domain from its
    supplemental qualifier (SUPP--) dataset."""
    return "SUPP" if str(spec.get("domain", "")).upper().startswith("SUPP") else "SDTM"


def mapping_row(col, index, spec, adam, input_names, code_by_var):
    """One variable-sheet row in the standard's column order."""
    name = col.get("name", "")
    label = col.get("label", "")
    sub = submission(col)
    origin = submission_origin(col) or classify_origin(
        col.get("derivation"), input_names
    )
    terms = controlled_terms(col, code_by_var)
    core = str(sub.get("core") or "")
    method = conversion_definition(col)
    if adam:
        return [
            str(spec.get("domain", "")),
            name,
            label,
            type_label(col.get("type"), True),
            terms,
            core,
            method,
            origin,
        ]
    length = sub.get("length")
    return [
        name,
        label,
        type_label(col.get("type"), False),
        "" if length is None else str(length),
        terms,
        origin,
        core,
        method,
        sdtm_variable_type(spec),
        str(index),
        define_comment(col),
    ]


def mapping_sheets(spec):
    """Sheet models: list of (tab id, tab label, headers, rows)."""
    columns = [c for c in spec.get("columns", []) if isinstance(c, dict)]
    input_names = set((spec.get("input") or {}).keys())
    output_names = (spec.get("output") or {}).get("columns") or [
        c["name"] for c in columns
    ]
    output_set = set(output_names)
    shown = [c for c in columns if c.get("name") in output_set]

    adam = is_adam(spec)
    code_by_var = dict(codelists(shown))
    map_headers = ADAM_HEADERS if adam else SDTM_HEADERS
    map_rows = [
        mapping_row(col, index, spec, adam, input_names, code_by_var)
        for index, col in enumerate(shown, start=1)
    ]

    sheets = [("mapping", "Mapping", map_headers, map_rows)]

    rows_spec = spec.get("rows") or []
    if rows_spec:
        template_vars = []
        for template in rows_spec:
            for var in template.get("derivations") or {}:
                if var not in template_vars:
                    template_vars.append(var)
        rc_rows = []
        for template in rows_spec:
            derivs = template.get("derivations") or {}
            rc_rows.append(
                [template.get("id", ""), template.get("filter", "")]
                + [
                    describe_row_template_derivation(derivs.get(v))
                    for v in template_vars
                ]
            )
        sheets.append(
            (
                "row-construction",
                "Row construction",
                ["Template", "Include when"] + template_vars,
                rc_rows,
            )
        )

    codes = codelists(shown)
    if codes:
        sheets.append(
            (
                "codelists",
                "Codelists",
                ["Variable", "Permitted values"],
                [[var, values] for var, values in codes],
            )
        )

    sheets.append(
        (
            "revision-history",
            "Revision history",
            ["Version", "Date", "Author", "Description", "Reviewer", "Sign-off"],
            [["1.0", "", "", "Initial generation from spec.yaml", "", ""]],
        )
    )
    return sheets


def render_table(tab_id, headers, rows):
    cells = "".join("<th>" + html.escape(h) + "</th>" for h in headers)
    body = []
    for row in rows:
        tds = "".join(
            '<td data-value="'
            + html.escape(str(v), quote=True)
            + '">'
            + html.escape(str(v))
            + "</td>"
            for v in row
        )
        body.append("<tr>" + tds + "</tr>")
    return (
        '<div class="table-scroll mapping-scroll" tabindex="0" role="region" '
        'aria-label="Mapping spec ' + html.escape(tab_id) + ' table">'
        '<table class="data-table mapping-table"><thead><tr>'
        + cells
        + "</tr></thead><tbody>"
        + "".join(body)
        + "</tbody></table></div>"
    )


def render_mapping_section(spec):
    """Full 'Mapping spec' section HTML with tabbed sheet panes."""
    sheets = mapping_sheets(spec)
    if not sheets:
        return ""
    tabs = []
    panes = []
    for position, (tab_id, label, headers, rows) in enumerate(sheets):
        selected = position == 0
        tabs.append(
            '<button type="button" role="tab" id="mapping-tab-' + tab_id + '" '
            'aria-controls="mapping-pane-'
            + tab_id
            + '" aria-selected="'
            + ("true" if selected else "false")
            + '" tabindex="'
            + ("0" if selected else "-1")
            + '" class="mapping-tab">'
            + html.escape(label)
            + "</button>"
        )
        panes.append(
            '<div role="tabpanel" id="mapping-pane-' + tab_id + '" '
            'aria-labelledby="mapping-tab-' + tab_id + '" tabindex="0" '
            'class="mapping-pane">' + render_table(tab_id, headers, rows) + "</div>"
        )
    script = MAPPING_TABS_JS_PATH.read_text(encoding="utf-8")
    return (
        '<section id="mapping-spec" class="panel mapping-panel" aria-labelledby="mapping-heading">'
        '<header class="panel-header"><span class="panel-title">'
        '<h2 id="mapping-heading">Mapping spec</h2></span>'
        '<span class="panel-caption">Generated from the YAML spec for human review; '
        "do not edit by hand</span></header>"
        '<div class="mapping-body">'
        '<div role="tablist" aria-label="Mapping spec sheets" class="mapping-tablist">'
        + "".join(tabs)
        + "</div>"
        + "".join(panes)
        + "</div></section>"
        "<script>" + script + "</script>"
    )
