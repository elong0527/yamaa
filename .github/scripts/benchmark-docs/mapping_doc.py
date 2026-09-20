#!/usr/bin/env python3
"""Render the human-review "Mapping spec" section of a benchmark dashboard.

The section is generated from the benchmark's spec.yaml: one row per output
column carrying the source variable(s), the derivation in plain language, the
origin, and the codelist binding. Rendering is deterministic: an unchanged
spec produces identical HTML.

Origin vocabulary follows Define-XML 2.1 OriginType (non-extensible):
Assigned, Collected, Derived, Not Available, Other, Predecessor, Protocol.
This file is ASCII-only to satisfy the repository source lint; arrows and
other non-ASCII glyphs are emitted as HTML character references.
"""

import html
from pathlib import Path

# ASCII escape sequences keep this file ASCII-clean for the repository source
# lint; the dashboard's final encode("ascii", "xmlcharrefreplace") turns them
# into HTML character references.
ARROW = "\u2192"
EMDASH = "\u2014"
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
    fallback = mapping.get("unmapped", mapping.get("missing"))
    tail = (
        "; missing or unlisted values " + ARROW + ' "' + str(fallback) + '"'
        if fallback is not None
        else ""
    )
    return "Recode " + str(var) + where + ": " + pairs + tail + "."


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
    groups = ", ".join(str(g) for g in window.get("group_by", []))
    return (
        '"Y" for the last record with '
        + str(node.get("date"))
        + " on or before "
        + str(node.get("reference_date"))
        + " within each ("
        + groups
        + "); blank otherwise."
    )


def describe_baseline_value(node):
    window = node.get("window", {})
    groups = ", ".join(str(g) for g in window.get("group_by", []))
    return (
        str(node.get("value"))
        + " from the record flagged "
        + str(node.get("flag"))
        + ' = "Y" within each ('
        + groups
        + ")."
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
    if "baseline_value" in derivation:
        return describe_baseline_value(derivation["baseline_value"])
    if "date_impute" in derivation:
        return describe_date_impute(derivation["date_impute"])
    return str(derivation)


def direct_sources(derivation):
    if isinstance(derivation, str):
        return [derivation]
    if not isinstance(derivation, dict):
        return []
    source = derivation.get("source")
    if isinstance(source, dict) and source.get("variable"):
        return [source["variable"]]
    mapping = derivation.get("mapping", {})
    src = mapping.get("source", {}) if isinstance(mapping, dict) else {}
    if isinstance(src, str):
        return [src]
    if src.get("variable"):
        return [src["variable"]]
    return []


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


def resolve_chain(name, col_by_name, input_names, seen=None):
    """One-level provenance chain, e.g. 'ARM (output column) -> ODM.Value
    (ODM.ItemOID = 'IT.DM.ARM')'."""
    seen = seen or set()
    if name in seen:
        return name
    seen = seen | {name}
    col = col_by_name.get(name)
    if col is None:
        return name
    derivation = col.get("derivation")
    if isinstance(derivation, str):
        if "." not in derivation:
            inner = resolve_chain(derivation, col_by_name, input_names, seen)
            return derivation + " (output column) " + ARROW + " " + inner
        return derivation
    return source_cell(col, col_by_name, input_names, seen)


def source_cell(col, col_by_name, input_names, seen=None):
    derivation = col.get("derivation")
    if isinstance(derivation, str):
        if "." in derivation and derivation.split(".")[0] in input_names:
            return derivation
        if "." not in derivation:
            inner = resolve_chain(derivation, col_by_name, input_names, seen)
            return derivation + " (output column) " + ARROW + " " + inner
        return derivation
    if not isinstance(derivation, dict):
        return EMDASH
    if "literal" in derivation:
        return EMDASH
    source = derivation.get("source")
    if isinstance(source, dict):
        var = str(source.get("variable", ""))
        return var + " (" + str(source["filter"]) + ")" if source.get("filter") else var
    mapping = derivation.get("mapping")
    if isinstance(mapping, dict):
        src = mapping.get("source", {})
        if isinstance(src, str):
            return src
        var = str(src.get("variable", ""))
        return var + " (" + str(src["filter"]) + ")" if src.get("filter") else var
    if "case" in derivation:
        branches = derivation["case"] or []
        cond = branches[0].get("when", "") if branches else ""
        ref = cond.split()[0] if cond else ""
        if ref and ref in col_by_name:
            inner = resolve_chain(ref, col_by_name, input_names, seen)
            return ref + " (output column) " + ARROW + " " + inner
        return ref or EMDASH
    if "compute" in derivation:
        return "output columns in expression"
    if "baseline_flag" in derivation:
        node = derivation["baseline_flag"]
        left = resolve_chain(str(node.get("date")), col_by_name, input_names, seen)
        right = resolve_chain(
            str(node.get("reference_date")), col_by_name, input_names, seen
        )
        return (
            str(node.get("date"))
            + " "
            + ARROW
            + " "
            + left
            + "; "
            + str(node.get("reference_date"))
            + " "
            + ARROW
            + " "
            + right
        )
    if "baseline_value" in derivation:
        node = derivation["baseline_value"]
        left = resolve_chain(str(node.get("value")), col_by_name, input_names, seen)
        right = resolve_chain(str(node.get("flag")), col_by_name, input_names, seen)
        return (
            str(node.get("value"))
            + " "
            + ARROW
            + " "
            + left
            + "; "
            + str(node.get("flag"))
            + " "
            + ARROW
            + " "
            + right
        )
    if "row_number" in derivation:
        order = derivation["row_number"].get("window", {}).get("order_by", [])
        return ", ".join(str(o) for o in order) or EMDASH
    if "date_impute" in derivation:
        return str(derivation["date_impute"].get("source"))
    return EMDASH


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


def mapping_sheets(spec):
    """Sheet models: list of (tab id, tab label, headers, rows)."""
    columns = [c for c in spec.get("columns", []) if isinstance(c, dict)]
    col_by_name = {c["name"]: c for c in columns if "name" in c}
    input_names = set((spec.get("input") or {}).keys())
    output_names = (spec.get("output") or {}).get("columns") or [
        c["name"] for c in columns
    ]
    output_set = set(output_names)
    shown = [c for c in columns if c.get("name") in output_set]

    sheets = []
    map_headers = [
        "#",
        "Target variable",
        "Label",
        "Type",
        "Source dataset",
        "Source variable(s)",
        "Mapping rule (plain language)",
        "Origin",
        "Controlled terminology",
        "Reviewer sign-off",
    ]
    code_by_var = dict(codelists(shown))
    map_rows = []
    for index, col in enumerate(shown, start=1):
        derivation = col.get("derivation")
        sources = [s for s in direct_sources(derivation) if s]
        datasets = sorted(
            {
                s.split(".")[0]
                for s in sources
                if "." in s and s.split(".")[0] in input_names
            }
        )
        map_rows.append(
            [
                str(index),
                col.get("name", ""),
                col.get("label", ""),
                col.get("type", ""),
                ", ".join(datasets) if datasets else EMDASH,
                source_cell(col, col_by_name, input_names),
                describe_derivation(derivation),
                classify_origin(derivation, input_names),
                code_by_var.get(col.get("name", ""), ""),
                "",
            ]
        )
    sheets.append(("mapping", "Mapping", map_headers, map_rows))

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
