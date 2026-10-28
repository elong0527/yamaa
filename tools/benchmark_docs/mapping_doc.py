"""Render the "Mapping spec" pane of a benchmark dashboard.

The pane is generated from the benchmark's spec.yaml and, when present, the
codelists of its study environment, as a familiar Excel specification: one row
per output column. The header layout follows the sponsor's SDTM and ADaM
variable sheets, so a reviewer reads the derived spec in the same shape they
author it. Rendering is deterministic: unchanged inputs produce identical HTML.

Values come from each column's `submission:` block when present -- `core`,
`length`, `codelist`, `data_type`, `origin.type`, `method`, `comment` -- and from
the study environment's codelists. The derivation supplies a plain-language
conversion rule and a Define-XML 2.1 origin classification (Assigned,
Collected, Derived, Not Available, Other, Predecessor, Protocol).

An expression field holds grammar text (EXPR-5, #3030 D18), which
`expression_node` reads into the node its describer reads, and YAML null is
the missing value, so a written handler is declared whatever it holds
(`declared`).

This file is ASCII-only to satisfy the repository source lint; arrows and
other non-ASCII glyphs are emitted as HTML character references.
"""

import html
import re

from tools import surface

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
        intermediates = spec.get("intermediates")
        self.lookups = {
            str(item.get("id")): item
            for item in (intermediates if isinstance(intermediates, list) else [])
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
        # Inputs whose every implicit join must find a record (LOOKUP-24);
        # the base is read as the row's own record, never joined.
        inputs = spec.get("input")
        self.required_inputs = {
            str(name)
            for name, declaration in (
                inputs.items() if isinstance(inputs, dict) else ()
            )
            if isinstance(declaration, dict)
            and declaration.get("require_match") is True
            and name != spec.get("base")
        }

    def lookup(self, name):
        return self.lookups.get(name)


EMPTY_CONTEXT = SpecContext()


def context(ctx):
    return ctx if isinstance(ctx, SpecContext) else EMPTY_CONTEXT


def expression_node(value):
    """An expression field's value in the shape the describers read.

    An expression field holds grammar text, where a bare name is a reference
    and quoted text a literal, or a YAML number, Boolean or null literal
    (EXPR-5, #3030 D18). A reference reads as its name, a literal as
    `{literal: X}`, a formula as `{compute: {expr: E}}` and an operation as
    its mapping.
    """
    kind, payload = surface.expression(value)
    if kind == "literal":
        return {"literal": payload}
    if kind == "formula":
        return {"compute": {"expr": payload}}
    return payload


def literal_text(value):
    """A declared literal as a reviewer reads it; the missing value is a blank cell.

    The literal is a value field's value or the `{literal: X}` node that
    `expression_node` reads; YAML null is the missing value (COLUMN-10).
    """
    if isinstance(value, dict) and set(value) == {"literal"}:
        value = value["literal"]
    return "blank" if value is None else '"' + str(value) + '"'


def missing_answer(value):
    """Whether a declared handler answers with the missing value."""
    return value is None or (
        isinstance(value, dict)
        and set(value) == {"literal"}
        and value["literal"] is None
    )


def declared(node, field):
    """Whether a handler or substitute field is declared: YAML null is the
    missing value, so a written field is declared whatever it holds (#3030
    D18)."""
    return isinstance(node, dict) and field in node


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
    else:
        # An omitted key matches on the output keys the donor dataset also
        # carries (LOOKUP-1), so the reviewer is told a join still happens.
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
    value = expression_node(value)
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
    one rather than bracketed like a code comment. `require_match: true` makes
    an unmatched row fatal (LOOKUP-24), which is the reviewable fact; without
    it the declared absence answer, `absent`, answers, or a blank when none
    is declared. A lookup reached through another lookup answers in the
    shorter form, so its clause cannot be mistaken for the outer lookup's own
    answer at the end of the sentence.
    """
    if isinstance(lookup, dict) and lookup.get("require_match") is True:
        return (
            ", an error when unmatched"
            if depth
            else ", and a row with no match is an error"
        )
    answer = literal_text(lookup["absent"]) if declared(lookup, "absent") else "blank"
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


def variable_name(variable, ctx=None):
    """A variable named again in the same rule: a lookup reference keeps only
    its column, its donor record having been spelt out at the first mention."""
    head, tail = split_reference(variable)
    if head and context(ctx).lookup(head) is not None:
        return tail
    return str(variable)


def value_phrase(derivation, ctx=None, depth=0):
    """A derivation as a noun phrase, for reuse inside a larger sentence.

    A describer writes a standalone rule ("Take QVAL from ..."), which reads
    wrong after "where ENDPOINT is"; the leading capital is what marks it as a
    sentence, so dropping it is all the repair needed.
    """
    derivation = expression_node(derivation)
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


REQUIRED_RECORD = "; a row with no matching record is an error"


def describe_reference(variable, ctx=None, depth=0):
    """A column that is one value copied in, from this row or another dataset."""
    head, tail = split_reference(variable)
    lookup = context(ctx).lookup(head) if head else None
    if lookup is None:
        required = head in context(ctx).required_inputs
        return sentence(
            "Copy value from " + str(variable) + (REQUIRED_RECORD if required else "")
        )
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


def handler_phrase(node, field, label, restated=False):
    """'; missing input -> "X"' for one declared handler, or nothing.

    `restated` marks a handler whose omission already gives the missing value
    (COLUMN-10): a declared missing answer there restates the default
    (P-HND-3), so the sheet does not state it.
    """
    if not declared(node, field):
        return ""
    if restated and missing_answer(node[field]):
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
    # A declared `absent` answers before any required match (LOOKUP-24), so
    # a missing answer is stated only where a required match is in effect.
    match_required = (
        src.get("require_match") is True or head in context(ctx).required_inputs
    )
    required = src.get("require_match") is True or (
        not declared(src, "absent") and head in context(ctx).required_inputs
    )
    return sentence(
        text
        + handler_phrase(src, "absent", "an absent source", restated=not match_required)
        + (REQUIRED_RECORD if required else "")
    )


def describe_mapping(mapping, ctx=None, depth=0):
    dictionary = mapping.get("dict", {})
    if isinstance(dictionary, str):
        # A dictionary kept in a project file is named rather than inlined.
        rule = " using the dictionary in " + dictionary
    else:
        # A null entry recodes to missing.
        rule = ": " + ", ".join(
            '"' + str(k) + '" ' + ARROW + " " + literal_text(v)
            for k, v in dictionary.items()
        )
    tail = mapped_answers(mapping)
    if not mapping.get("case_sensitive", True):
        tail += "; matching ignores letter case"
    return "Recode " + mapped_source(mapping, ctx, depth) + rule + tail + "."


def describe_decode(decode, ctx=None, depth=0):
    """The decode a codelist's item gives each coded value (EXPR-15)."""
    return (
        "Decode "
        + mapped_source(decode, ctx, depth)
        + " through codelist "
        + str(decode.get("codelist"))
        + mapped_answers(decode)
        + "."
    )


def mapped_source(node, ctx=None, depth=0):
    """The variable a `mapping` or a `decode` reads, with its filter."""
    src = node.get("source", {})
    if isinstance(src, str):
        var, filt = src, None
    else:
        var = src.get("variable")
        filt = src.get("filter")
    where = " where " + str(filt) if filt else ""
    return describe_variable(var, ctx, depth) + where


def mapped_answers(node):
    """What a missing and an unlisted value of a `mapping` or a `decode`
    yield, as one clause when they agree."""
    absent = describe_mapping_handler(node, "missing")
    unlisted = describe_mapping_handler(node, "unmapped")
    if absent == unlisted:
        return "; missing or unlisted values " + absent
    return "; missing values " + absent + "; unlisted values " + unlisted


def describe_mapping_handler(mapping, handler):
    """What one mapping event yields: a literal, missing, or an error.

    Each event has its own handler. An omitted `missing` gives the missing
    value and an omitted `unmapped` fails (COLUMN-10).
    """
    if not declared(mapping, handler):
        return "stay missing" if handler == "missing" else "are errors"
    value = mapping[handler]
    if isinstance(value, dict) and set(value) == {"literal"}:
        value = value["literal"]
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
    result = expression_node(result)
    if isinstance(result, dict):
        if "literal" in result:
            return '"' + str(result["literal"]) + '"'
        return value_phrase(result, ctx, depth)
    return '"' + str(result) + '"'


def describe_flag(node, ctx=None, depth=0):
    """`flag` is the one-branch `case` that yields a flag value (EXPR-8)."""
    if not isinstance(node, dict):
        return (
            '"Y" when '
            + describe_expression_text(node, ctx, depth)
            + "; blank otherwise."
        )
    condition = describe_expression_text(node.get("condition"), ctx, depth)
    # A written null is the missing value; an omitted `true_value` is "Y".
    true_value = literal_text(
        node["true_value"] if declared(node, "true_value") else "Y"
    )
    false_value = literal_text(node.get("false_value"))
    text = true_value + " when " + condition + "; " + false_value + " otherwise"
    # An unknown condition never falls through to false_value (EXPR-8).
    text += "; " + literal_text(node.get("missing")) + " when the condition is unknown"
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
    return sentence(
        text + handler_phrase(node, "missing", "all missing", restated=True)
    )


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
        # A lone formula is the shorthand for {expr: ...}.
        node = {"expr": node}
    text = "Aggregate " + describe_expression_text(node.get("expr"), ctx, depth)
    clauses = []
    groups = node.get("group_by")
    if groups:
        clauses.append("grouped by " + ", ".join(str(g) for g in groups))
    clauses.extend(match_clauses(node, ctx, depth))
    # `derivations` maps each binding's name to its type and derivation.
    derive = [
        {"name": name, **binding}
        for name, binding in (node.get("derivations") or {}).items()
        if isinstance(binding, dict)
    ]
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
            text += " " + label + join_and([str(n) for n in value])
    if node.get("filter"):
        text += " where " + str(node["filter"])
    return sentence(text + "; no identified record yields blank")


def describe_function(node):
    args = []
    for name, value in (node.get("args") or {}).items():
        # An argument is one grammar leaf: a name, or a literal written as
        # quoted or temporal text (FUNC-6).
        kind, payload = surface.expression(value)
        if kind != "operation":
            value = payload
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
    right = node.get("closed") == "right"
    closed = (
        "intervals closed on the right" if right else "intervals closed on the left"
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
    return sentence(
        text + handler_phrase(node, "missing", "a missing source", restated=True)
    )


def is_datetime(node):
    """The temporal verbs read their declared `type`, never the data."""
    return node.get("type") == "datetime"


def describe_diff(node, ctx=None, depth=0):
    """Whole calendar units between two dates, or the elapsed time between
    two datetimes."""
    unit = str(node.get("unit", ""))
    start = describe_variable(node.get("start"), ctx, depth)
    end = describe_variable(node.get("end"), ctx, depth)
    if is_datetime(node):
        text = (
            ("Whole seconds" if unit == "second" else "Elapsed " + unit + "s")
            + " from "
            + start
            + " to "
            + end
            + ", negative when "
            + end
            + " is earlier"
        )
        return sentence(text if unit == "second" else text + ", not rounded")
    # Endpoint inclusion is spelled `closed` (P-ORD-2).
    closed = str(node.get("closed") or "right")
    bounds = {"right": "exclusive", "both": "inclusive", "neither": "between"}.get(
        closed, closed
    )
    counted = {
        "exclusive": "counting the end date but not the start",
        "inclusive": "counting both endpoints",
        "between": "counting neither endpoint",
    }.get(bounds, "counting " + bounds)
    return sentence("Whole " + unit + "s from " + start + " to " + end + ", " + counted)


def describe_shift(node, ctx=None, depth=0):
    """A date moved by whole days, or a datetime by an elapsed amount.

    Worded as the value the column holds, so it reads the same as a rule and
    inside a case branch.
    """
    if is_datetime(node):
        noun, amount, unit = "datetime", node.get("amount"), str(node.get("unit", ""))
    else:
        noun, amount, unit = "date", node.get("days"), "day"
    source = describe_variable(node.get("source"), ctx, depth)
    if isinstance(amount, bool) or not isinstance(amount, (int, float)):
        # A variable amount is signed, so adding it moves either way.
        return sentence(
            "The "
            + noun
            + " "
            + source
            + " plus "
            + describe_variable(amount, ctx, depth)
            + " "
            + unit
            + "s"
        )
    if amount == 0:
        return sentence("The same " + noun + " as " + source)
    size = abs(amount)
    return sentence(
        "The "
        + noun
        + " "
        + str(size)
        + " "
        + unit
        + ("" if size == 1 else "s")
        + (" after " if amount > 0 else " before ")
        + source
    )


def describe_study_day(node, ctx=None, depth=0):
    return sentence(
        "CDISC study day of "
        + describe_variable(node.get("source"), ctx, depth)
        + " against "
        + describe_variable(node.get("reference"), ctx, depth)
        + " as day 1; there is no day zero"
    )


def describe_to_date(node, ctx=None, depth=0):
    # A comma, not a semicolon: the rule often sits inside a case branch.
    return sentence(
        "Take the date part of "
        + describe_variable(node.get("source"), ctx, depth)
        + ", blank for a year or a year and month alone"
    )


# The spellings each date form reads, as the sheet names them (TIME-22).
DATE_FORM_SPELLINGS = {
    "iso": "ISO 8601 date text",
    "ddmonyyyy": "day-month-year text such as 04MAR2001",
    "mdy_slash": "month-first slashed dates such as 03/04/2001 for March 4",
    "dmy_slash": "day-first slashed dates such as 03/04/2001 for April 3",
    "yyyymmdd": "eight digits such as 20010304",
    "monyyyy": "month-year text such as MAR2001 for a month alone",
}


def describe_date_text(node, ctx=None, depth=0):
    """Text in one of the declared spellings read as ISO 8601 date text."""
    forms = node.get("forms")
    spellings = [
        DATE_FORM_SPELLINGS.get(str(form), str(form))
        for form in (forms if isinstance(forms, list) else [])
    ]
    if len(spellings) > 1:
        listed = ", ".join(spellings[:-1]) + " or " + spellings[-1]
    else:
        listed = spellings[0] if spellings else "no declared spelling"
    text = (
        "Read "
        + describe_variable(node.get("source"), ctx, depth)
        + " as ISO 8601 date text from "
        + listed
        + "; blank when it is blank"
    )
    if declared(node, "invalid"):
        text += handler_phrase(node, "invalid", "any other spelling")
    else:
        text += "; any other spelling is an error"
    return sentence(text)


def describe_to_epoch_day(node, ctx=None, depth=0):
    return sentence(
        "Days since 1970-01-01 for " + describe_variable(node.get("source"), ctx, depth)
    )


# The parts dtc writes, coarsest first.
DATE_PARTS = ("year", "month", "day", "hour", "minute", "second")


def describe_dtc(node, ctx=None, depth=0):
    """Collected date parts written as ISO 8601 text, cut at the first blank."""
    parts = ", ".join(
        part + " " + describe_variable(node.get(part), ctx, depth)
        for part in DATE_PARTS
        if part in node
    )
    text = (
        "Write the collected "
        + parts
        + " as ISO 8601 date text, stopping before the first blank part; "
        + "blank when the year is blank"
    )
    if declared(node, "invalid"):
        text += handler_phrase(node, "invalid", "a part that is not its digits")
    else:
        text += "; a part that is not its digits is an error"
    return sentence(text)


def describe_time_of_day(node, ctx=None, depth=0):
    return sentence(
        "Seconds since midnight for the time of day of "
        + describe_variable(node.get("source"), ctx, depth)
    )


def describe_duration_text(node, ctx=None, depth=0):
    """A number of elapsed units written as ISO 8601 duration text."""
    return sentence(
        "Write "
        + describe_variable(node.get("source"), ctx, depth)
        + ", a number of "
        + str(node.get("unit", ""))
        + "s, as ISO 8601 duration text with days as the largest unit"
    )


def invalid_text_phrase(node, ctx=None):
    """What text that is not a date yields; with no handler it is fatal."""
    if "invalid" in node:
        return handler_phrase(node, "invalid", "invalid text")
    return "; invalid text is an error"


# The collected precision words, finest first. A date's precision stops at
# day; "none" belongs to a value taken whole from an imputation fallback and
# to text whose year is unknown, such as --03-15 (TIME-7).
PRECISION_WORDS = {
    "date": (
        '"day" for a complete date, "month" for a year and month, "year" for '
        'a year alone, "none" for an entirely imputed date or text with an '
        "unknown year"
    ),
    "datetime": (
        '"second" through the second, "minute" through the minute, "hour" '
        'through the hour, "day" for a date alone, "month" for a year and '
        'month, "year" for a year alone, "none" for an entirely imputed '
        "datetime or text with an unknown year"
    ),
}


def describe_precision(node, ctx=None, depth=0):
    kind = "datetime" if is_datetime(node) else "date"
    text = (
        "Collected "
        + kind
        + " precision of "
        + describe_variable(node.get("source"), ctx, depth)
        + ": "
        + PRECISION_WORDS[kind]
    )
    return sentence(text + invalid_text_phrase(node, ctx))


# What a minimum source precision leaves uncompleted; year, the default,
# completes every truncated source.
UNCOMPLETED_SOURCES = {
    "month": "a year alone gives blank",
    "day": "a source without a complete date gives blank",
}


def describe_impute(node, ctx=None, depth=0):
    """Truncated collected text completed to the declared type.

    A bound moves only what imputation supplied, so one outside the collected
    period is ignored, and not_after wins when the two bounds cross.
    """
    kind = "datetime" if is_datetime(node) else "date"
    text = (
        "Impute incomplete "
        + kind
        + " from "
        + describe_variable(node.get("source"), ctx, depth)
    )
    defaults = [
        field + "=" + str(node[field])
        for field in ("month", "day")
        if node.get(field) is not None
    ]
    if defaults:
        text += ", defaulting " + ", ".join(defaults)
    if kind == "datetime":
        edge = "earliest" if str(node.get("time", "")) == "first" else "latest"
        text += ", giving each time part the source lacks its " + edge + " value"
    before = node.get("not_before")
    after = node.get("not_after")
    if before:
        text += ", never before " + describe_variable(before, ctx, depth)
    if after:
        text += ", never after " + describe_variable(after, ctx, depth)
    if before or after:
        text += "; a bound outside the collected period is ignored"
    if before and after:
        text += (
            ", and "
            + variable_name(after, ctx)
            + " wins over a later "
            + variable_name(before, ctx)
        )
    # The declared month and day state the minimum source precision
    # (TIME-10, P-IMP-1).
    minimum = (
        "year"
        if node.get("month") is not None
        else "month"
        if node.get("day") is not None
        else "day"
    )
    uncompleted = UNCOMPLETED_SOURCES.get(str(minimum))
    if uncompleted:
        text += "; " + uncompleted
    if node.get("fallback"):
        text += (
            "; a missing source takes "
            + describe_variable(node["fallback"], ctx, depth)
            + ", counted as entirely imputed"
        )
    return sentence(text + invalid_text_phrase(node, ctx))


# The ADaM date (--DTF) and time (--TMF) imputation flag codes, read from the
# collected precision of the imputed value.
IMPUTATION_FLAG_CODES = {
    "date": (
        '"Y" when the whole date was imputed, "M" when the month and day '
        'were, "D" when only the day was; blank when the date was collected '
        "in full"
    ),
    "time": (
        '"H" when the whole time was imputed, "M" when the minutes and '
        'seconds were, "S" when only the seconds were; blank when the time '
        "was collected to the second"
    ),
}
# A study that does not collect seconds never imputes them alone.
MINUTE_TIME_FLAG_CODES = (
    '"H" when the whole time was imputed, "M" when the minutes were; blank '
    "when the minutes were collected, since the study does not collect seconds"
)


def describe_imputation_flag(node, ctx=None, depth=0):
    part = "time" if node.get("part") == "time" else "date"
    codes = IMPUTATION_FLAG_CODES[part]
    if part == "time" and node.get("seconds_collected", True) is False:
        codes = MINUTE_TIME_FLAG_CODES
    return sentence(
        part.capitalize()
        + " imputation flag for "
        + describe_variable(node.get("source"), ctx, depth)
        + ": "
        + codes
    )


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
    # A declared list replaces the characters after which proper case starts
    # a word (TEXT-5).
    if declared(node, "delimiters"):
        text += ", starting a word after any of " + literal_text(node["delimiters"])
    return sentence(
        text + handler_phrase(node, "missing", "a missing source", restated=True)
    )


def describe_str_pad(node, ctx=None, depth=0):
    return sentence(
        "Pad "
        + describe_variable(node.get("source"), ctx, depth)
        + " on the left with spaces to at least "
        + str(node.get("width"))
        + " characters"
    )


def describe_str_substr(node, ctx=None, depth=0):
    start, length = node.get("start"), node.get("length")

    def whole(value):
        return isinstance(value, int) and not isinstance(value, bool)

    if length is None:
        part = "characters " + str(start) + " to the end"
    elif whole(start) and whole(length):
        part = "characters " + str(start) + " to " + str(start + length - 1)
    else:
        part = str(length) + " characters from character " + str(start)
    # A start past the text's last character gives a blank (TEXT-13).
    blank = (
        "it is empty"
        if start == 1
        else "it has fewer than " + str(start) + " characters"
    )
    text = (
        "Take "
        + part
        + " of "
        + describe_variable(node.get("source"), ctx, depth)
        + ", blank when "
        + blank
    )
    return sentence(
        text + handler_phrase(node, "missing", "a missing source", restated=True)
    )


def describe_str_length(node, ctx=None, depth=0):
    text = "Number of characters in " + describe_variable(
        node.get("source"), ctx, depth
    )
    return sentence(
        text + handler_phrase(node, "missing", "a missing source", restated=True)
    )


def describe_str_decimal(node, ctx=None, depth=0):
    decimals = node.get("decimals")
    if decimals == 0:
        places = "no decimals"
    else:
        places = (
            "exactly " + str(decimals) + (" decimal" if decimals == 1 else " decimals")
        )
    text = (
        "Write "
        + describe_variable(node.get("source"), ctx, depth)
        + " as text with "
        + places
        + ", rounded half away from zero"
    )
    if node.get("width") is not None:
        text += (
            ", padded on the left with spaces to at least "
            + str(node["width"])
            + " characters"
        )
    return sentence(
        text + handler_phrase(node, "missing", "a missing source", restated=True)
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
    text += handler_phrase(node, "missing", "a missing source", restated=True)
    return sentence(text + handler_phrase(node, "no_match", "no match"))


def describe_str_replace(node, ctx=None, depth=0):
    replacement = node.get("replacement", "")
    text = (
        "Replace every match of "
        + str(node.get("pattern"))
        + " in "
        + describe_variable(node.get("source"), ctx, depth)
        + (" with " + literal_text(replacement) if replacement else " with nothing")
    )
    return sentence(
        text + handler_phrase(node, "missing", "a missing source", restated=True)
    )


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


def describe_locf(node, ctx=None, depth=0):
    source = describe_variable(node.get("source"), ctx, depth)
    if node.get("include_current", True) is False:
        text = "Most recent non-missing " + source + " from an earlier row"
    else:
        text = "Last observation carried forward from " + source
    return sentence(with_window(text, node, ctx, depth))


def describe_baseline_flag(node, ctx=None, depth=0):
    window = node.get("window", {})
    scope = (
        " using window " + window
        if isinstance(window, str)
        else " within each group of "
        + ", ".join(str(g) for g in window.get("group_by", []))
    )
    # The eligible rows are ordered by `window.order_by`, the date alone by
    # default (P-OP-2, P-OP-3, P-ORD-1).
    source, reference = node.get("source"), node.get("reference")
    order = window.get("order_by") if isinstance(window, dict) else None
    terms = order or [node.get("date")]
    value = " and a non-missing " + str(source) if source else ""
    bound = " before " if node.get("inclusive") is False else " on or before "
    ordering = order_terms(terms)
    return (
        '"Y" for the last record with '
        + str(node.get("date"))
        + bound
        + str(reference)
        + value
        + scope
        + ", ordered by "
        + ordering
        + "; blank otherwise."
    )


# Shape name -> describer. A shape absent here renders its YAML, which is the
# signal that a new expression needs a sentence.
DERIVATION_DESCRIBERS = {
    "literal": lambda n, c, d: describe_literal(n),
    "source": describe_source,
    "mapping": describe_mapping,
    "decode": describe_decode,
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
    "diff": describe_diff,
    "shift": describe_shift,
    "impute": describe_impute,
    "precision": describe_precision,
    "imputation_flag": describe_imputation_flag,
    "to_date": describe_to_date,
    "date_text": describe_date_text,
    "to_epoch_day": describe_to_epoch_day,
    "dtc": describe_dtc,
    "time_of_day": describe_time_of_day,
    "duration_text": describe_duration_text,
    "study_day": describe_study_day,
    "str_case": describe_str_case,
    "str_pad": describe_str_pad,
    "str_substr": describe_str_substr,
    "str_length": describe_str_length,
    "str_decimal": describe_str_decimal,
    "str_extract": describe_str_extract,
    "str_replace": describe_str_replace,
    "str_template": describe_str_template,
    "row_number": describe_row_number,
    "rank": describe_rank,
    "row_value": describe_row_value,
    "locf": describe_locf,
    "baseline_flag": describe_baseline_flag,
}


def describe_derivation(derivation, ctx=None, depth=0):
    """Plain-language mapping rule for one column-level derivation."""
    derivation = expression_node(derivation)
    if isinstance(derivation, str):
        return describe_reference(derivation, ctx, depth)
    if not isinstance(derivation, dict):
        return str(derivation)
    # `handled_expression_class`: an inner expression plus the value a failed
    # conversion yields (SUB-1).
    if "value" in derivation:
        text = clause(describe_derivation(derivation["value"], ctx, depth))
        if declared(derivation, "unconvertible"):
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

    SUB-15 admits only Derived, Assigned, Predecessor and Other for the
    adam family, so a bare copy from a declared input is Predecessor and a
    recode or any other computation is Derived. For sdtm the same copy is
    Collected; a mapping or source over a locally derived column computes
    rather than copies, so it is Derived.
    """
    lookups = context(ctx).lookups
    if derivation is not None:
        derivation = expression_node(derivation)
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
        "ADAECM",
        "ADCE",
        "ADCM",
        "ADEG",
        "ADEX",
        "ADEXSUM",
        "ADFA",
        "ADICE",
        "ADINC",
        "ADIS",
        "ADLB",
        "ADLBC",
        "ADOE",
        "ADPC",
        "ADPP",
        "ADQS",
        "ADRS",
        "ADRISK",
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
    """The authored origin types, which a column lists (P-ORIGIN-1)."""
    origins = submission(col).get("origins")
    types = [
        str(item.get("type") or "")
        for item in (origins if isinstance(origins, list) else [])
        if isinstance(item, dict)
    ]
    return ", ".join(t for t in types if t)


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
    if isinstance(declared, str) and declared:
        return declared
    # A non-string `type` (a list where the type name belongs) is unhashable,
    # so the mapping lookup itself would raise TypeError; it names no known
    # type, so it resolves to no submission type.
    col_type = col.get("type")
    if not isinstance(col_type, str):
        return ""
    return {"str": "text", "int": "integer"}.get(col_type, col_type)


def controlled_terms(col, code_by_var, definitions):
    """The named terminology, permitted values, or ISO 8601 format."""
    code = submission(col).get("codelist")
    if code:
        # A codelist id is a string; an unhashable value cannot be looked
        # up, so it renders as written, like an unknown id below.
        if not isinstance(code, str):
            return str(code)
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
    """Fallback terminology when a benchmark declares no codelists."""
    out = []
    for col in columns:
        checks = col.get("verifications") or []
        if not isinstance(checks, list):
            checks = []
        for check in checks:
            if isinstance(check, dict) and "allowed_values" in check:
                allowed = check["allowed_values"]
                values = allowed.get("values", []) if isinstance(allowed, dict) else []
                if not isinstance(values, list):
                    values = []
                out.append((col.get("name", ""), ", ".join(str(v) for v in values)))
    return out


def document_codelists(define):
    """The study environment's codelists in declaration order."""
    if not isinstance(define, dict):
        return []
    codelists = define.get("codelists")
    return [
        item
        for item in (codelists if isinstance(codelists, list) else [])
        if isinstance(item, dict)
    ]


def document_codelist_rows(definitions):
    """One row per declared value, or one row for an external codelist."""
    rows = []
    for definition in definitions:
        external = definition.get("external")
        external = external if isinstance(external, dict) else {}
        items = definition.get("items")
        items = items if isinstance(items, list) and items else [None]
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
    derivation = expression_node(derivation)
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
        "1.7",
        "2026-10-20",
        (
            "Read a specification in the 1.0 surface as its 0.5 counterpart; "
            "state that missing values stay missing where a mapping declares no "
            "missing handler, and leave out a declared missing answer that "
            "restates a handler's default"
        ),
    ),
    (
        "1.6",
        "2026-10-17",
        (
            "Describe collected date parts written as ISO 8601 date text and "
            "times of day counted in seconds since midnight"
        ),
    ),
    (
        "1.5",
        "2026-10-17",
        (
            "Name collected precision in words instead of letter codes; state "
            "imputation fallbacks, minimum source precision, bounds ignored "
            "outside the collected period, and invalid text with no handler "
            "as an error; describe date and datetime shifts, imputation flag "
            "codes, and partial text giving no date part"
        ),
    ),
    (
        "1.4",
        "2026-10-15",
        (
            "State datetime precision codes through the hour and minute, and "
            "complete only the time parts a collected datetime lacks"
        ),
    ),
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
    # A derivation written as YAML null is the missing literal (D18).
    written = "derivation" in col
    origin = submission_origin(col) or classify_origin(
        expression_node(col.get("derivation")) if written else None,
        input_names,
        ctx,
        adam,
    )
    terms = controlled_terms(col, code_by_var, definitions)
    core = str(sub.get("core") or "")
    method = conversion_definition(col, ctx)
    if not written and not submission(col).get("method"):
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
    spec = spec if isinstance(spec, dict) else {}
    # A field written with no value parses as None, not as an absent key, so
    # a bare .get(..., []) default does not apply to it; the other fields in
    # this function (and SpecContext above) already guard with `or` /
    # isinstance, and a malformed field must render an empty sheet instead
    # of crashing the whole dashboard build with a TypeError/KeyError.
    # Only a list is a valid `columns` value: a truthy scalar (a count or a
    # flag written where the list belongs) is not iterable, so the `or []`
    # guard alone still let it escape as a TypeError.
    raw_columns = spec.get("columns")
    columns = (
        [c for c in raw_columns if isinstance(c, dict)]
        if isinstance(raw_columns, list)
        else []
    )
    # Normalize `rows` before SpecContext reads it: a scalar `rows` value
    # would otherwise raise TypeError inside SpecContext before the sheet
    # filter below ever runs, and a non-mapping `derivations` value would
    # abort row construction when it is read with .get().
    raw_rows = spec.get("rows")
    rows_spec = (
        [
            {
                **template,
                "derivations": template.get("derivations")
                if isinstance(template.get("derivations"), dict)
                else {},
            }
            for template in raw_rows
            if isinstance(template, dict)
        ]
        if isinstance(raw_rows, list)
        else []
    )
    ctx = SpecContext({**spec, "rows": rows_spec})
    inputs = spec.get("input")
    input_names = set(inputs) if isinstance(inputs, dict) else set()
    output = spec.get("output")
    declared_output = output.get("columns") if isinstance(output, dict) else None
    # Only a list of column names is a valid `output.columns`; a string
    # would splinter into characters and a list of mappings is unhashable,
    # so anything else falls back to the named columns.
    output_names = (
        declared_output
        if isinstance(declared_output, list)
        and all(isinstance(name, str) for name in declared_output)
        else [
            c.get("name")
            for c in columns
            if isinstance(c.get("name"), str) and c.get("name")
        ]
    )
    output_set = set(output_names)
    # A column name is an identifier, so a non-string `name` (a list or a
    # mapping where the identifier belongs) can never match the declared
    # output; testing it against the set would raise TypeError on the
    # unhashable value instead of omitting the column.
    shown = [
        c
        for c in columns
        if isinstance(c.get("name"), str) and c.get("name") in output_set
    ]

    adam = is_adam(spec)
    fallback_codes = allowed_value_codelists(shown)
    code_by_var = dict(fallback_codes)
    document_codes = document_codelists(define)
    definitions = {
        item["id"]: item
        for item in document_codes
        if isinstance(item.get("id"), str) and item.get("id")
    }
    map_headers = ADAM_HEADERS if adam else SDTM_HEADERS
    map_rows = [
        mapping_row(col, index, spec, adam, input_names, code_by_var, definitions, ctx)
        for index, col in enumerate(shown, start=1)
    ]

    sheets = [("mapping", "Mapping", map_headers, map_rows)]

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
                    # A variable that only another template fills is absent
                    # here, unlike a written null, which is a constant.
                    describe_row_template_derivation(derivs[v], ctx)
                    if v in derivs
                    else "None"
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
