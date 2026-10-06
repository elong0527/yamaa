#!/usr/bin/env python3
"""Render the "Mapping spec" pane of a benchmark dashboard.

The pane is generated from the benchmark's spec.yaml and, when present,
define.yaml as a familiar Excel specification: one row per output column. The
header layout follows the sponsor's SDTM and ADaM variable sheets, so a
reviewer reads the derived spec in the same shape they author it. Rendering is
deterministic: unchanged inputs produce identical HTML.

Values come from each column's `submission:` block when present -- `core`,
`length`, `codelist`, `data_type`, `origin.type`, `method`, `comment` -- and from
the study document's codelists. The derivation supplies a plain-language
conversion rule and a Define-XML 2.1 origin classification (Assigned,
Collected, Derived, Not Available, Other, Predecessor, Protocol).

This file is ASCII-only to satisfy the repository source lint; arrows and
other non-ASCII glyphs are emitted as HTML character references.
"""

import html
import re

# ASCII escape sequences keep this file ASCII-clean for the repository source
# lint; the dashboard's final encode("ascii", "xmlcharrefreplace") turns them
# into HTML character references.
ARROW = "\u2192"

# Sentence templates: one plain-language mapping rule per derivation shape.
#
# A reviewer reads the value a column ends up holding, not the pipeline that
# produced it. A reference into an `intermediates:` lookup is therefore
# resolved in place: the sentence names the donor record -- its dataset, join
# key, range, filter, ordering, and no-match value -- and never prints the
# intermediate's id, which is an internal handle with no counterpart in a
# sponsor's Excel specification.

# A lookup whose donor record is itself selected through another lookup would
# nest without bound; two levels is as deep as a readable sentence goes, and
# below that a reference keeps only its column name.
MAX_LOOKUP_DEPTH = 2

# A qualified reference inside a predicate or formula. The leading identifier
# rules out a decimal number, and a quoted literal carries no bare period, so
# only real DATASET.COLUMN references match.
REFERENCE_PATTERN = re.compile(
    r"\b([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)\b"
)


class SpecContext:
    """The lookups a derivation is read against.

    Built once per spec so each describer can resolve an
    `INTERMEDIATE.COLUMN` reference without threading the whole spec through
    every call. Describers called without one -- on a bare node, as the tests
    do -- leave such references exactly as the spec writes them.
    """

    def __init__(self, spec=None):
        spec = spec or {}
        self.lookups = {
            str(item.get("id")): item
            for item in (spec.get("intermediates") or [])
            if isinstance(item, dict) and item.get("id")
        }
        # Columns a `rows:` template fills, which carry no derivation of
        # their own on the variable sheet.
        self.row_template_columns = {
            str(name)
            for template in (spec.get("rows") or [])
            if isinstance(template, dict)
            for name in (template.get("derivations") or {})
        }

    def lookup(self, name):
        return self.lookups.get(name)


EMPTY_CONTEXT = SpecContext()


def context(ctx):
    return ctx if isinstance(ctx, SpecContext) else EMPTY_CONTEXT


def literal_text(value):
    """A declared literal as a reviewer reads it; a null handler is a blank cell."""
    return "blank" if value is None else '"' + str(value) + '"'


def join_and(parts):
    """'A', 'A and B', 'A, B and C' -- never a trailing comma that reads as a
    fourth item when the parts themselves contain commas."""
    parts = [str(p) for p in parts if str(p)]
    if len(parts) < 2:
        return "".join(parts)
    return ", ".join(parts[:-1]) + " and " + parts[-1]


def listed(lead, parts):
    """'<lead> A and B'; items carrying their own commas are separated by
    semicolons after a colon instead, so the reader sees where each one ends."""
    parts = [str(p) for p in parts if str(p)]
    if any("," in p for p in parts):
        return lead + ": " + "; ".join(parts)
    return lead + " " + join_and(parts)


def join_then(parts):
    return " then ".join(str(p) for p in parts if str(p))


def sentence(text):
    """One rule, punctuated once."""
    text = str(text).strip()
    return text if text.endswith(".") else text + "."


def clause(text):
    """A described expression reused mid-sentence, with its full stop dropped."""
    return str(text).rstrip(".")


def order_terms(terms):
    """Ordering terms; direction and null placement stated only when declared."""
    out = []
    for term in terms or []:
        if isinstance(term, dict):
            text = str(term.get("variable", ""))
            if term.get("direction") == "desc":
                text += " descending"
            if term.get("nulls"):
                text += ", nulls " + str(term["nulls"])
        else:
            text = str(term)
        out.append(text)
    return join_then(out)


def split_reference(text):
    """('MEDDRA_CODING', 'PTNAME') for a qualified reference, else (None, text)."""
    if isinstance(text, str) and "." in text:
        head, _, tail = text.partition(".")
        return head, tail
    return None, text


# --- Lookups rendered as the donor record they select ----------------------


def between_clause(between):
    """An inclusive range match, worded for dates and numbers alike."""
    value = str(between.get("value", ""))
    lower = between.get("lower")
    upper = between.get("upper")
    if lower and upper:
        return "where " + value + " falls within " + str(lower) + " to " + str(upper)
    if lower:
        return "where " + value + " is " + str(lower) + " or more"
    return "where " + value + " is " + str(upper) + " or less"


def match_clauses(node, ctx=None, depth=0):
    """Join key, range, and filter conditions selecting the donor records."""
    clauses = []
    key = node.get("key")
    if isinstance(key, dict):
        clauses.append(
            "whose "
            + join_and(
                [
                    str(column) + " equals " + match_value(value, ctx, depth)
                    for column, value in key.items()
                ]
            )
        )
    elif isinstance(key, list):
        clauses.append("matching on " + join_and([str(k) for k in key]))
    elif isinstance(key, str):
        clauses.append("matching on " + key)
    else:
        # An omitted key matches on the output keys the donor dataset also
        # carries (REQ-0150), so the reviewer is told a join still happens.
        clauses.append("matching on the shared output keys")
    between = node.get("between")
    if isinstance(between, dict):
        clauses.append(between_clause(between))
    if node.get("filter"):
        clauses.append("where " + describe_expression_text(node["filter"], ctx, depth))
    return clauses


# A join key may be computed rather than read, and "IDVARVAL equals pad
# LB.LBSEQ" puts an imperative where the sentence wants a thing. These give the
# shapes that can sit on the key side of a match a noun form instead.
KEY_VALUE_PHRASES = {
    "literal": lambda n, c, d: literal_text(n),
    "str_pad": lambda n, c, d: (
        describe_variable(n.get("source"), c, d)
        + " padded on the left with spaces to at least "
        + str(n.get("width"))
        + " characters"
    ),
    "str_case": lambda n, c, d: (
        describe_variable(n.get("source"), c, d) + " in " + str(n.get("to")) + " case"
    ),
    "compute": lambda n, c, d: describe_expression_text(n.get("expr"), c, d),
}


def match_value(value, ctx=None, depth=0):
    """The current-row side of one join key: a variable, or a computed value."""
    if not isinstance(value, dict):
        return describe_variable(value, ctx, depth)
    for shape, phrase in KEY_VALUE_PHRASES.items():
        if shape in value:
            return phrase(value[shape], ctx, depth)
    # Any other shape keeps its own rule wording; "the value" carries the
    # imperative without stranding it mid-sentence.
    return "the value from " + value_phrase(value, ctx, depth)


def no_match_phrase(lookup, depth=0):
    """What an unmatched row yields, as a trailing clause.

    A sponsor's variable sheet is written in sentences, so this is worded as
    one rather than bracketed like a code comment. An absent `no_match` makes
    an unmatched row fatal (REQ-0124), which is the reviewable fact -- not an
    omission to render as a blank cell. A lookup reached through another
    lookup answers in the shorter form, so its clause cannot be mistaken for
    the outer lookup's own answer at the end of the sentence.
    """
    if "no_match" not in lookup:
        return (
            ", an error when unmatched"
            if depth
            else ", and a row with no match is an error"
        )
    answer = literal_text(lookup["no_match"])
    if depth:
        return ", " + answer + " when unmatched"
    return ", and no match yields " + answer


def record_phrase(lookup, column=None, ctx=None, depth=0):
    """The donor record a lookup selects, named by its dataset.

    `column`, when it is one of the lookup's own `derivations`, is explained in
    place so the reviewer sees where a value with no stored counterpart in the
    donor dataset comes from.
    """
    dataset = str(lookup.get("dataset") or "")
    keep = lookup.get("keep")
    position = str(keep) + " " if keep else ""
    if dataset.upper() == "SELF":
        noun = "the " + position + "already-derived row of this dataset"
    else:
        noun = "the " + position + dataset + " record"
    clauses = match_clauses(lookup, ctx, depth)
    ordering = order_terms(lookup.get("order_by"))
    if ordering:
        clauses.append("ordered by " + ordering)
    derivations = lookup.get("derivations") or {}
    if column in derivations and depth < MAX_LOOKUP_DEPTH:
        clauses.append(
            "where "
            + str(column)
            + " is derived as "
            + value_phrase(derivations[column], ctx, depth)
        )
    return noun + (" " + ", ".join(clauses) if clauses else "")


def describe_variable(variable, ctx=None, depth=0):
    """One variable slot, with a lookup reference resolved to its donor record."""
    if not isinstance(variable, str):
        return str(variable)
    head, tail = split_reference(variable)
    lookup = context(ctx).lookup(head) if head else None
    if lookup is None:
        return variable
    if depth >= MAX_LOOKUP_DEPTH:
        return tail
    return (
        tail
        + " from "
        + record_phrase(lookup, tail, ctx, depth + 1)
        + no_match_phrase(lookup, depth)
    )


def value_phrase(derivation, ctx=None, depth=0):
    """A derivation as a noun phrase, for reuse inside a larger sentence.

    A describer writes a standalone rule ("Take QVAL from ..."), which reads
    wrong after "where ENDPOINT is"; the leading capital is what marks it as a
    sentence, so dropping it is all the repair needed.
    """
    if isinstance(derivation, str):
        return describe_variable(derivation, ctx, depth)
    text = clause(describe_derivation(derivation, ctx, depth))
    return text[:1].lower() + text[1:] if text else text


def describe_expression_text(text, ctx=None, depth=0):
    """A predicate or formula with its lookup references made readable.

    Spelling a donor record out where the reference sits would bury the
    condition, so the reference keeps only its column name and the record it
    came from is named once, after the expression.
    """
    ctx = context(ctx)
    if not isinstance(text, str) or not ctx.lookups:
        return str(text)
    seen = []

    def swap(match):
        head, tail = match.group(1), match.group(2)
        if ctx.lookup(head) is None:
            return match.group(0)
        if (head, tail) not in seen:
            seen.append((head, tail))
        return tail

    rewritten = REFERENCE_PATTERN.sub(swap, text)
    if not seen or depth >= MAX_LOOKUP_DEPTH:
        return rewritten
    origins = [
        tail
        + " taken from "
        + record_phrase(ctx.lookup(head), tail, ctx, depth + 1)
        + no_match_phrase(ctx.lookup(head), depth)
        for head, tail in seen
    ]
    return rewritten + ", with " + join_and(origins)


def describe_reference(variable, ctx=None, depth=0):
    """A column that is one value copied in, from this row or another dataset."""
    head, tail = split_reference(variable)
    lookup = context(ctx).lookup(head) if head else None
    if lookup is None:
        return "Copy value from " + str(variable) + "."
    return sentence(
        "Take "
        + tail
        + " from "
        + record_phrase(lookup, tail, ctx, depth + 1)
        + no_match_phrase(lookup, depth)
    )


def describe_window(window, ctx=None, depth=0):
    """A window clause: a named window, or its partition and ordering inline."""
    if isinstance(window, str):
        return "using window " + window
    if not isinstance(window, dict):
        return ""
    parts = []
    groups = window.get("group_by")
    if groups:
        parts.append("within each group of " + ", ".join(str(g) for g in groups))
    ordering = order_terms(window.get("order_by"))
    if ordering:
        parts.append("ordered by " + ordering)
    if window.get("filter"):
        parts.append("over rows where " + str(window["filter"]))
    return ", ".join(parts)


def with_window(text, node, ctx=None, depth=0):
    window = describe_window(node.get("window"), ctx, depth)
    return text + (" " + window if window else "")


def handler_phrase(node, field, label):
    """'; missing input -> "X"' for one declared handler, or nothing."""
    if field not in node:
        return ""
    return "; " + label + " " + ARROW + " " + literal_text(node[field])


# --- One describer per derivation shape ------------------------------------


# A column can reach the variable sheet with no derivation of its own: a
# `rows:` template supplies it per constructed row, or an inherited parent
# layer does. Either way the cell says where to look instead of printing
# Python's "None".
ROW_TEMPLATE_RULE = "Set per constructed row; see the Row construction sheet."
INHERITED_RULE = "No derivation in this specification; inherited from its parent."


def no_derivation_rule(column_name, ctx=None):
    if column_name in context(ctx).row_template_columns:
        return ROW_TEMPLATE_RULE
    return INHERITED_RULE


def describe_literal(value):
    return 'Set constant value "' + str(value) + '".'


def describe_source(src, ctx=None, depth=0):
    """A `source:` binding: the variable, plus any record selection it declares."""
    if isinstance(src, str):
        return describe_reference(src, ctx, depth)
    variable = src.get("variable")
    head, _ = split_reference(variable)
    lookup = context(ctx).lookup(head) if head else None
    if lookup is not None:
        return describe_reference(variable, ctx, depth)
    selection = []
    if src.get("filter"):
        selection.append("where " + str(src["filter"]))
    ordering = order_terms(src.get("order_by"))
    if ordering:
        keep = src.get("keep")
        selection.append(
            ("keeping the " + str(keep) + " " if keep else "")
            + "ordered by "
            + ordering
        )
    text = "Copy value from " + str(variable)
    if selection:
        text += " " + ", ".join(selection)
    return sentence(text + handler_phrase(src, "absent", "an absent source"))


def describe_mapping(mapping, ctx=None, depth=0):
    src = mapping.get("source", {})
    if isinstance(src, str):
        var, filt = src, None
    else:
        var = src.get("variable")
        filt = src.get("filter")
    where = " where " + str(filt) if filt else ""
    dictionary = mapping.get("dict", {})
    if isinstance(dictionary, str):
        # A dictionary kept in a project file is named rather than inlined.
        rule = " using the dictionary in " + dictionary
    else:
        rule = ": " + ", ".join(
            '"' + str(k) + '" ' + ARROW + ' "' + str(v) + '"'
            for k, v in dictionary.items()
        )
    absent = describe_mapping_handler(mapping, "missing")
    unlisted = describe_mapping_handler(mapping, "unmapped")
    if absent == unlisted:
        tail = "; missing or unlisted values " + absent
    else:
        tail = "; missing values " + absent + "; unlisted values " + unlisted
    if not mapping.get("case_sensitive", True):
        tail += "; matching ignores letter case"
    return "Recode " + describe_variable(var, ctx, depth) + where + rule + tail + "."


def describe_mapping_handler(mapping, handler):
    """What one mapping event yields: a literal, missing, or an error.

    Each event has its own handler, and an omitted handler makes the event
    fatal (REQ-0344).
    """
    if handler not in mapping:
        return "are errors"
    value = mapping[handler]
    return "stay missing" if value is None else ARROW + ' "' + str(value) + '"'


def describe_case(branches, ctx=None, depth=0):
    """A branch ladder; an `otherwise:` branch carries the value in its own key."""
    parts = []
    has_otherwise = False
    for branch in branches or []:
        if not isinstance(branch, dict):
            continue
        if "otherwise" in branch:
            has_otherwise = True
            parts.append(
                "otherwise " + describe_case_result(branch["otherwise"], ctx, depth)
            )
        else:
            parts.append(
                "If "
                + describe_expression_text(branch.get("when"), ctx, depth)
                + " then "
                + describe_case_result(branch.get("then"), ctx, depth)
            )
    if not has_otherwise:
        parts.append("otherwise blank")
    return "; ".join(parts) + "."


def describe_case_result(result, ctx=None, depth=0):
    """A branch result: a literal, or a nested expression described in place."""
    if isinstance(result, dict):
        if "literal" in result:
            return '"' + str(result["literal"]) + '"'
        return value_phrase(result, ctx, depth)
    return '"' + str(result) + '"'


def describe_flag(node, ctx=None, depth=0):
    """`flag` is the one-branch `case` that yields a flag value (REQ-1256)."""
    if not isinstance(node, dict):
        return (
            '"Y" when '
            + describe_expression_text(node, ctx, depth)
            + "; blank otherwise."
        )
    condition = describe_expression_text(node.get("condition"), ctx, depth)
    true_value = literal_text(node.get("true_value", "Y"))
    false_value = literal_text(node.get("false_value"))
    text = true_value + " when " + condition + "; " + false_value + " otherwise"
    # An unknown condition never falls through to false_value (REQ-1257).
    if "missing" in node:
        text += "; " + literal_text(node["missing"]) + " when the condition is unknown"
    else:
        text += "; blank when the condition is unknown"
    return sentence(text)


def describe_first_available(node, ctx=None, depth=0):
    sources = []
    for src in node.get("sources") or []:
        if isinstance(src, dict):
            text = describe_variable(src.get("variable"), ctx, depth)
            if src.get("filter"):
                text += " where " + str(src["filter"])
            sources.append(text)
        else:
            sources.append(describe_variable(src, ctx, depth))
    text = listed("First non-missing of", sources)
    return sentence(text + handler_phrase(node, "missing", "all missing"))


def describe_extreme(node, word, ctx=None, depth=0):
    return sentence(
        listed(
            word + " of",
            [describe_variable(s, ctx, depth) for s in node.get("sources") or []],
        )
    )


def describe_aggregate(node, ctx=None, depth=0):
    """A summary over donor records, with the records it summarises named."""
    if not isinstance(node, dict):
        return "Aggregate " + describe_expression_text(node, ctx, depth) + "."
    text = "Aggregate " + describe_expression_text(node.get("expr"), ctx, depth)
    clauses = []
    groups = node.get("group_by")
    if groups:
        clauses.append("grouped by " + ", ".join(str(g) for g in groups))
    clauses.extend(match_clauses(node, ctx, depth))
    derive = node.get("derive") or []
    for binding in derive:
        if isinstance(binding, dict):
            clauses.append(
                "with "
                + str(binding.get("name"))
                + " as "
                + value_phrase(binding.get("derivation"), ctx, depth)
            )
    if clauses:
        text += " over records " + ", ".join(clauses)
    return sentence(text)


def describe_odm(node):
    """One collected item read from the row's ODM scope."""
    if isinstance(node, str):
        node = {"item": node}
    if not isinstance(node, dict):
        return str(node)
    _, item = split_reference(node.get("item"))
    text = "Read collected ODM item " + str(item)
    for field, label in (
        ("event", "in event "),
        ("form", "on form "),
        ("item_group", "in item group "),
    ):
        value = node.get(field)
        if value:
            names = value if isinstance(value, list) else [value]
            text += " " + label + join_and([str(n) for n in names])
    if node.get("filter"):
        text += " where " + str(node["filter"])
    return sentence(text + "; no identified record yields blank")


def describe_function(node):
    args = []
    for name, value in (node.get("args") or {}).items():
        if isinstance(value, dict):
            value = (
                value.get("literal")
                if "literal" in value
                else value.get("date", value.get("datetime"))
            )
        args.append(str(name) + " = " + str(value))
    text = "Call project function " + str(node.get("name"))
    version = node.get("contract_version")
    if version:
        text += " at contract version " + str(version)
    if args:
        text += " with " + join_and(args)
    return sentence(text)


def describe_cut(node, ctx=None, depth=0):
    breaks = [str(b) for b in node.get("breaks") or []]
    labels = ['"' + str(label) + '"' for label in node.get("labels") or []]
    closed = (
        "intervals closed on the right"
        if node.get("right")
        else "intervals closed on the left"
    )
    text = (
        "Band "
        + describe_variable(node.get("source"), ctx, depth)
        + " at "
        + ", ".join(breaks)
        + " into "
        + join_and(labels)
        + ", "
        + closed
    )
    return sentence(text + handler_phrase(node, "missing", "a missing source"))


def describe_date_diff(node, ctx=None, depth=0):
    unit = str(node.get("unit", ""))
    bounds = str(node.get("bounds") or "exclusive")
    counted = {
        "exclusive": "counting the end date but not the start",
        "inclusive": "counting both endpoints",
        "between": "counting neither endpoint",
    }.get(bounds, "counting " + bounds)
    return sentence(
        "Whole "
        + unit
        + "s from "
        + describe_variable(node.get("start"), ctx, depth)
        + " to "
        + describe_variable(node.get("end"), ctx, depth)
        + ", "
        + counted
    )


def describe_study_day(node, ctx=None, depth=0):
    return sentence(
        "CDISC study day of "
        + describe_variable(node.get("date"), ctx, depth)
        + " against "
        + describe_variable(node.get("reference"), ctx, depth)
        + " as day 1; there is no day zero"
    )


def describe_to_date(node, ctx=None, depth=0):
    return sentence(
        "Take the date part of " + describe_variable(node.get("source"), ctx, depth)
    )


def describe_to_epoch_day(node, ctx=None, depth=0):
    return sentence(
        "Days since 1970-01-01 for " + describe_variable(node.get("source"), ctx, depth)
    )


def describe_precision(node, word, ctx=None, depth=0):
    text = (
        "Collected "
        + word
        + " precision of "
        + describe_variable(node.get("source"), ctx, depth)
        + ": D for a complete date, M for year and month, Y for a year alone"
    )
    text += handler_phrase(node, "missing", "a missing source")
    return sentence(text + handler_phrase(node, "invalid", "invalid text"))


def describe_date_impute(node, ctx=None, depth=0):
    bits = []
    if node.get("month") is not None:
        bits.append("month=" + str(node["month"]))
    if node.get("day") is not None:
        bits.append("day=" + str(node["day"]))
    text = (
        "Impute incomplete date from "
        + describe_variable(node.get("source"), ctx, depth)
        + ", defaulting "
        + ", ".join(bits)
    )
    if node.get("not_before"):
        text += ", never before " + str(node["not_before"])
    text += handler_phrase(node, "missing", "a missing source")
    text += handler_phrase(node, "invalid", "invalid text")
    if "missing" not in node and "invalid" not in node:
        text += "; missing or invalid input yields null"
    return sentence(text)


def describe_datetime_impute(node, ctx=None, depth=0):
    time = str(node.get("time", ""))
    instant = (
        "the first instant of the day"
        if time == "first"
        else "the last instant of the day"
    )
    text = (
        "Impute incomplete datetime from "
        + describe_variable(node.get("source"), ctx, depth)
        + ", defaulting the time to "
        + instant
    )
    text += handler_phrase(node, "missing", "a missing source")
    return sentence(text + handler_phrase(node, "invalid", "invalid text"))


def describe_round(node, ctx=None, depth=0):
    digits = node.get("digits")
    unit = " place" if digits in (1, -1) else " places"
    place = (
        str(digits) + " decimal" + unit
        if isinstance(digits, int) and digits >= 0
        else str(abs(digits) if isinstance(digits, int) else digits)
        + unit
        + " left of the decimal point"
    )
    return sentence(
        "Round "
        + describe_variable(node.get("source"), ctx, depth)
        + " to "
        + place
        + ", ties away from zero"
    )


def describe_compute(node, ctx=None, depth=0):
    return "Compute " + describe_expression_text(node.get("expr"), ctx, depth) + "."


def describe_str_case(node, ctx=None, depth=0):
    to = str(node.get("to", ""))
    text = (
        "Convert "
        + describe_variable(node.get("source"), ctx, depth)
        + " to "
        + to
        + " case"
    )
    return sentence(text + handler_phrase(node, "missing", "a missing source"))


def describe_str_pad(node, ctx=None, depth=0):
    return sentence(
        "Pad "
        + describe_variable(node.get("source"), ctx, depth)
        + " on the left with spaces to at least "
        + str(node.get("width"))
        + " characters"
    )


def describe_str_extract(node, ctx=None, depth=0):
    group = node.get("group", 0)
    part = "the whole match" if group in (0, None) else "capture group " + str(group)
    text = (
        "Extract "
        + part
        + " matching "
        + str(node.get("pattern"))
        + " from "
        + describe_variable(node.get("source"), ctx, depth)
    )
    text += handler_phrase(node, "missing", "a missing source")
    return sentence(text + handler_phrase(node, "no_match", "no match"))


def describe_str_contains(node, ctx=None, depth=0):
    text = (
        "True when "
        + describe_variable(node.get("source"), ctx, depth)
        + " matches "
        + str(node.get("pattern"))
        + " anywhere, false when it does not"
    )
    return sentence(text + handler_phrase(node, "missing", "a missing source"))


def describe_str_concat(node, ctx=None, depth=0):
    parts = []
    for part in node.get("sources") or []:
        if isinstance(part, dict) and "literal" in part:
            parts.append('"' + str(part["literal"]) + '"')
        elif isinstance(part, dict) and "source" in part:
            parts.append(describe_variable(part["source"], ctx, depth))
        elif isinstance(part, dict):
            parts.append(value_phrase(part, ctx, depth))
        else:
            parts.append(describe_variable(part, ctx, depth))
    text = "Concatenate " + join_and(parts)
    return sentence(text + handler_phrase(node, "missing", "a missing part"))


def describe_str_template(node, ctx=None, depth=0):
    if isinstance(node, str):
        node = {"template": node}
    text = 'Fill the template "' + str(node.get("template")) + '"'
    return sentence(text + handler_phrase(node, "missing", "a missing field"))


def describe_row_number(node, ctx=None, depth=0):
    """Kept as its own sentence: the window words differ from every other shape."""
    window = node.get("window", {})
    if isinstance(window, str):
        return "Row number using window " + window + "."
    groups = ", ".join(str(g) for g in window.get("group_by", []))
    order = order_terms(window.get("order_by"))
    text = "Row number within each group of " + groups
    if order:
        text += ", ordered by " + order
    return text + "."


def describe_rank(node, ctx=None, depth=0):
    method = str(node.get("method") or "competition")
    tie = (
        "ties share a number and the next distinct value skips the gap"
        if method == "competition"
        else "ties share a number and the next distinct value follows without a gap"
    )
    return sentence(with_window("Rank rows from 1", node, ctx, depth) + "; " + tie)


def describe_row_value(node, ctx=None, depth=0):
    offset = node.get("offset", 0)
    try:
        step = int(offset)
    except (TypeError, ValueError):
        step = 0
    if step < 0:
        position = str(-step) + " row" + ("s" if step < -1 else "") + " earlier"
    elif step > 0:
        position = str(step) + " row" + ("s" if step > 1 else "") + " later"
    else:
        position = "the same row"
    text = (
        "Take "
        + describe_variable(node.get("source"), ctx, depth)
        + " from "
        + position
    )
    return sentence(with_window(text, node, ctx, depth))


def describe_previous_non_missing(node, ctx=None, depth=0):
    text = (
        "Most recent non-missing "
        + describe_variable(node.get("source"), ctx, depth)
        + " from an earlier row"
    )
    return sentence(with_window(text, node, ctx, depth))


def describe_locf(node, ctx=None, depth=0):
    text = "Last observation carried forward from " + describe_variable(
        node.get("source"), ctx, depth
    )
    return sentence(with_window(text, node, ctx, depth))


def describe_baseline_flag(node, ctx=None, depth=0):
    window = node.get("window", {})
    scope = (
        " using window " + window
        if isinstance(window, str)
        else " within each group of "
        + ", ".join(str(g) for g in window.get("group_by", []))
    )
    return (
        '"Y" for the last record with '
        + str(node.get("date"))
        + " on or before "
        + str(node.get("reference_date"))
        + scope
        + "; blank otherwise."
    )


# Shape name -> describer. A shape absent here renders its YAML, which is the
# signal that a new expression needs a sentence.
DERIVATION_DESCRIBERS = {
    "literal": lambda n, c, d: describe_literal(n),
    "source": describe_source,
    "mapping": describe_mapping,
    "case": describe_case,
    "flag": describe_flag,
    "first_available": describe_first_available,
    "greatest": lambda n, c, d: describe_extreme(n, "Greatest", c, d),
    "least": lambda n, c, d: describe_extreme(n, "Least", c, d),
    "aggregate": describe_aggregate,
    "odm": lambda n, c, d: describe_odm(n),
    "function": lambda n, c, d: describe_function(n),
    "cut": describe_cut,
    "compute": describe_compute,
    "round_half_away_from_zero": describe_round,
    "date_diff": describe_date_diff,
    "date_impute": describe_date_impute,
    "date_precision": lambda n, c, d: describe_precision(n, "date", c, d),
    "datetime_impute": describe_datetime_impute,
    "datetime_precision": lambda n, c, d: describe_precision(n, "datetime", c, d),
    "to_date": describe_to_date,
    "to_epoch_day": describe_to_epoch_day,
    "study_day": describe_study_day,
    "str_case": describe_str_case,
    "str_pad": describe_str_pad,
    "str_extract": describe_str_extract,
    "str_contains": describe_str_contains,
    "str_concat": describe_str_concat,
    "str_template": describe_str_template,
    "row_number": describe_row_number,
    "rank": describe_rank,
    "row_value": describe_row_value,
    "previous_non_missing": describe_previous_non_missing,
    "locf": describe_locf,
    "baseline_flag": describe_baseline_flag,
}


def describe_derivation(derivation, ctx=None, depth=0):
    """Plain-language mapping rule for one column-level derivation."""
    if isinstance(derivation, str):
        return describe_reference(derivation, ctx, depth)
    if not isinstance(derivation, dict):
        return str(derivation)
    # `handled_expression_class`: an inner expression plus the value a failed
    # conversion yields (REQ-1148).
    if "value" in derivation:
        text = clause(describe_derivation(derivation["value"], ctx, depth))
        if "unconvertible" in derivation:
            text += (
                "; a value that cannot be converted "
                + ARROW
                + " "
                + literal_text(derivation["unconvertible"])
            )
        return sentence(text)
    for shape, describer in DERIVATION_DESCRIBERS.items():
        if shape in derivation:
            return describer(derivation[shape], ctx, depth)
    return str(derivation)


def classify_origin(derivation, input_names, ctx=None, adam=False):
    """Origin per the Define-XML 2.1 vocabulary for one standard family.

    A reference through an `intermediates:` lookup is Derived however the
    donor column was collected: the value this dataset publishes is the
    product of the join, not a field a site typed into this record.

    REQ-0890 admits only Derived, Assigned, Predecessor and Other for the
    adam family, so a bare copy from a declared input is Predecessor and a
    recode or any other computation is Derived. For sdtm the same copy is
    Collected; a mapping or source over a locally derived column computes
    rather than copies, so it is Derived.
    """
    lookups = context(ctx).lookups
    if isinstance(derivation, str):
        base = derivation.split(".")[0]
        if base in lookups:
            return "Derived"
        if base in input_names:
            return "Predecessor" if adam else "Collected"
        return "Derived"
    if not isinstance(derivation, dict):
        return "Derived"
    if "literal" in derivation:
        return "Assigned"
    if "mapping" in derivation:
        if adam:
            return "Derived"
        mapping = derivation.get("mapping")
        variable = None
        if isinstance(mapping, dict):
            src = mapping.get("source")
            if isinstance(src, str):
                variable = src
            elif isinstance(src, dict):
                variable = src.get("variable")
        head, _ = split_reference(variable)
        if head in lookups:
            return "Derived"
        return "Collected" if head in input_names else "Derived"
    source = derivation.get("source")
    if source is not None:
        variable = source if isinstance(source, str) else source.get("variable")
        head, _ = split_reference(variable)
        if head in lookups:
            return "Derived"
        if head in input_names:
            return "Predecessor" if adam else "Collected"
        return "Derived"
    if "odm" in derivation:
        odm = derivation.get("odm")
        variable = None
        if isinstance(odm, str):
            variable = odm
        elif isinstance(odm, dict):
            variable = odm.get("item")
        head, _ = split_reference(variable)
        if head in lookups:
            return "Derived"
        if head in input_names:
            return "Predecessor" if adam else "Collected"
        return "Derived"
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
        "ADFA",
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


TEMPORAL_SUBMISSION_TYPES = frozenset(
    {
        "date",
        "datetime",
        "time",
        "partialDate",
        "partialTime",
        "partialDatetime",
        "incompleteDate",
        "incompleteTime",
        "incompleteDatetime",
        "durationDatetime",
        "intervalDatetime",
    }
)


def resolved_data_type(col):
    """Submission data type, including the default from the column type."""
    declared = submission(col).get("data_type")
    if declared:
        return declared
    return {"str": "text", "int": "integer"}.get(col.get("type"), col.get("type"))


def controlled_terms(col, code_by_var, definitions):
    """The named terminology, permitted values, or ISO 8601 format."""
    code = submission(col).get("codelist")
    if code:
        definition = definitions.get(code)
        if definition is None:
            return str(code)
        name = str(definition.get("name") or code)
        external = definition.get("external")
        if isinstance(external, dict):
            dictionary = str(external.get("dictionary") or "")
            version = external.get("version")
            details = dictionary + (f", version {version}" if version else "")
            if details:
                return name + " (" + details + ")"
        return name
    values = code_by_var.get(col.get("name", ""))
    if values:
        return values
    return "ISO 8601" if resolved_data_type(col) in TEMPORAL_SUBMISSION_TYPES else ""


def conversion_definition(col, ctx=None):
    """The authored `submission.method`, else the derivation in plain language."""
    method = submission(col).get("method")
    if method:
        return str(method)
    return describe_derivation(col.get("derivation"), ctx)


def define_comment(col):
    comment = submission(col).get("comment")
    if isinstance(comment, dict):
        return str(comment.get("text") or "")
    return str(comment) if comment else ""


def allowed_value_codelists(columns):
    """Fallback terminology when a benchmark has no study document."""
    out = []
    for col in columns:
        checks = col.get("verifications") or []
        if isinstance(checks, dict):
            checks = [checks]
        for check in checks:
            if isinstance(check, dict) and "allowed_values" in check:
                values = check["allowed_values"].get("values", [])
                out.append((col["name"], ", ".join(str(v) for v in values)))
    return out


def document_codelists(define):
    """The study document's codelists in declaration order."""
    if not isinstance(define, dict):
        return []
    return [
        item for item in define.get("codelists", []) or [] if isinstance(item, dict)
    ]


def document_codelist_rows(definitions):
    """One row per declared value, or one row for an external codelist."""
    rows = []
    for definition in definitions:
        external = definition.get("external")
        external = external if isinstance(external, dict) else {}
        items = definition.get("items") or [None]
        for item in items:
            item = item if isinstance(item, dict) else {}
            rows.append(
                [
                    definition.get("id", ""),
                    definition.get("name", ""),
                    item.get("value", ""),
                    item.get("decode", ""),
                    definition.get("alias", ""),
                    item.get("alias", ""),
                    "Yes" if definition.get("extensible", False) else "No",
                    "Yes" if item.get("extended", False) else "",
                    definition.get("format_name", ""),
                    external.get("dictionary", ""),
                    external.get("version", ""),
                ]
            )
    return rows


def describe_row_template_derivation(derivation, ctx=None):
    """A row-template cell: terser than a variable-sheet rule, but never raw YAML."""
    if isinstance(derivation, str):
        return "Copy " + describe_variable(derivation, ctx)
    if isinstance(derivation, dict):
        if "literal" in derivation:
            return 'Constant "' + str(derivation["literal"]) + '"'
        if "source" in derivation:
            source = derivation["source"]
            variable = source if isinstance(source, str) else source.get("variable")
            return "Copy " + describe_variable(variable, ctx)
        compute = derivation.get("compute", {})
        if isinstance(compute, dict) and compute.get("expr"):
            return "Compute " + str(compute["expr"])
        return clause(describe_derivation(derivation, ctx))
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


REVISION_HISTORY_HEADERS = [
    "Version",
    "Date",
    "Author",
    "Description",
    "Reviewer",
    "Sign-off",
]

# The Mapping spec is generated, never authored, so its revision history
# tracks the renderer rather than the benchmark's spec.yaml: an unchanged spec
# can still read differently after a wording change, and a reviewer who signed
# off on the earlier wording needs to see that it moved. Newest first; add an
# entry whenever a change alters the text of a rendered cell. Reviewer and
# Sign-off stay empty -- those are the reviewer's to fill, not the build's.
RENDERER_REVISIONS = [
    (
        "1.3",
        "2026-09-29",
        (
            "Show study codelist names, external dictionaries, and ISO 8601 "
            "formats in variable rows; list the document's codelist values "
            "and metadata"
        ),
    ),
    (
        "1.2",
        "2026-09-29",
        (
            "Classify ADaM copies from a declared input as Predecessor and "
            "recodes as Derived, and SDTM recodes of locally derived "
            "columns as Derived"
        ),
    ),
    (
        "1.1",
        "2026-09-29",
        (
            "Resolve lookups into the donor record they select, and give every "
            "derivation shape a plain-language rule instead of its raw YAML"
        ),
    ),
    ("1.0", "", "Initial generation from spec.yaml"),
]

RENDERER_AUTHOR = "yamaa docs build"


def revision_history_rows():
    """The renderer's own history, identical across benchmarks by design."""
    return [
        [version, date, RENDERER_AUTHOR, description, "", ""]
        for version, date, description in RENDERER_REVISIONS
    ]


def sdtm_variable_type(spec):
    """The Variable Type cell distinguishes a parent SDTM domain from its
    supplemental qualifier (SUPP--) dataset."""
    return "SUPP" if str(spec.get("domain", "")).upper().startswith("SUPP") else "SDTM"


def mapping_row(
    col, index, spec, adam, input_names, code_by_var, definitions, ctx=None
):
    """One variable-sheet row in the standard's column order."""
    name = col.get("name", "")
    label = col.get("label", "")
    sub = submission(col)
    origin = submission_origin(col) or classify_origin(
        col.get("derivation"), input_names, ctx, adam
    )
    terms = controlled_terms(col, code_by_var, definitions)
    core = str(sub.get("core") or "")
    method = conversion_definition(col, ctx)
    if col.get("derivation") is None and not submission(col).get("method"):
        method = no_derivation_rule(name, ctx)
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


def mapping_sheets(spec, define=None):
    """Sheet models: list of (tab id, tab label, headers, rows)."""
    columns = [c for c in spec.get("columns", []) if isinstance(c, dict)]
    ctx = SpecContext(spec)
    input_names = set((spec.get("input") or {}).keys())
    output_names = (spec.get("output") or {}).get("columns") or [
        c["name"] for c in columns
    ]
    output_set = set(output_names)
    shown = [c for c in columns if c.get("name") in output_set]

    adam = is_adam(spec)
    fallback_codes = allowed_value_codelists(shown)
    code_by_var = dict(fallback_codes)
    document_codes = document_codelists(define)
    definitions = {item["id"]: item for item in document_codes if item.get("id")}
    map_headers = ADAM_HEADERS if adam else SDTM_HEADERS
    map_rows = [
        mapping_row(col, index, spec, adam, input_names, code_by_var, definitions, ctx)
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
                    describe_row_template_derivation(derivs.get(v), ctx)
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

    if define is not None and document_codes:
        sheets.append(
            (
                "codelists",
                "Codelists",
                [
                    "Codelist ID",
                    "Name",
                    "Value",
                    "Decode",
                    "Codelist NCI alias",
                    "Value NCI alias",
                    "Extensible",
                    "Extended value",
                    "SAS format",
                    "Dictionary",
                    "Version",
                ],
                document_codelist_rows(document_codes),
            )
        )
    elif define is None and fallback_codes:
        sheets.append(
            (
                "codelists",
                "Codelists",
                ["Variable", "Permitted values"],
                [[var, values] for var, values in fallback_codes],
            )
        )

    sheets.append(
        (
            "revision-history",
            "Revision history",
            REVISION_HISTORY_HEADERS,
            revision_history_rows(),
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


def render_mapping_sheets(spec, define=None):
    """The 'Mapping spec' pane's sheets: list of (sheet id, label, table HTML)."""
    return [
        (sheet_id, label, render_table(sheet_id, headers, rows))
        for sheet_id, label, headers, rows in mapping_sheets(spec, define)
    ]
