"""Negative contracts: the spec paths and condition of `expected/error.yaml`.

The only change #3030 section 1 permits in an expected file is a spec path
rewritten through the rename table's `paths` block, so each entry of that
block has one rule here, applied in a fixed order, and a condition the
table's `conditions` block renames is renamed (`rename_condition`). A
negative whose fault changes under a decided item (D60, D61) is listed for
independent re-authoring instead, as the block's `negative_benchmarks` notes
say; this module never edits one.
"""

import re

from tools.migrate.convert import IDENTITY
from tools.migrate.text import path_segment

SECTIONS = ("sdtm", "adam", "send")
_INDEXED = re.compile(r"^(intermediates|rows)\[(\d+)\]$")
_SEGMENTS = re.compile(r"\['(?:[^']|'')*'\]|\[\d+\]|[^.\[\]]+")


def segments_of(path):
    return _SEGMENTS.findall(path)


def joined(segments):
    out = ""
    for segment in segments:
        out += segment if segment.startswith("[") else ("." if out else "") + segment
    return out


def _navigate(value, segments):
    """The spec value a path's leading segments name, members by identity."""
    current = value
    for position, segment in enumerate(segments):
        if segment.startswith("["):
            index = int(segment[1:-1])
            if not isinstance(current, list) or index >= len(current):
                return None
            current = current[index]
        elif isinstance(current, list) and position == 1 and segments[0] in IDENTITY:
            identity = IDENTITY[segments[0]]
            current = next(
                (
                    item
                    for item in current
                    if isinstance(item, dict) and str(item.get(identity)) == segment
                ),
                None,
            )
        elif isinstance(current, dict):
            current = current.get(segment)
        else:
            return None
    return current


def _rename_after(segments, owner, old, new):
    out = list(segments)
    for index in range(len(out) - 1):
        if out[index] == owner and out[index + 1] == old:
            out[index + 1] = new
    return out


def rule_indexed(table, spec, segments, cleared):
    """intermediates[N] and rows[N] -> intermediates.<id> and rows.<id> (P-CLR-2)."""
    if len(segments) < 2:
        return segments
    match = _INDEXED.match(segments[0] + segments[1])
    if not match:
        return segments
    container, index = match.group(1), int(match.group(2))
    members = spec.get(container) if isinstance(spec, dict) else None
    if (
        not isinstance(members, list)
        or index >= len(members)
        or not isinstance(members[index], dict)
    ):
        return None
    identity = members[index].get(IDENTITY[container])
    if identity is None:
        return None
    return [container, path_segment(str(identity))] + segments[2:]


def rule_compute(table, spec, segments, cleared):
    """derivation.compute.expr and derivation.compute -> derivation (D18)."""
    out = []
    for index, segment in enumerate(segments):
        if segment == "compute":
            continue
        if segment == "expr" and index and segments[index - 1] == "compute":
            continue
        out.append(segment)
    return out


def rule_literal(table, spec, segments, cleared):
    """<field>.literal -> <field> (D18)."""
    return [segment for segment in segments if segment != "literal"]


def rule_intermediate_no_match(table, spec, segments, cleared):
    """intermediates[N].no_match -> intermediates.<id>.absent (P-HND-1)."""
    if (
        len(segments) >= 3
        and segments[0] == "intermediates"
        and segments[2] == "no_match"
    ):
        return (
            segments[:2]
            + [table.new_name("spec.intermediate_class.no_match")]
            + segments[3:]
        )
    return segments


def rule_read_no_match(table, spec, segments, cleared):
    """<read>.no_match in a run report's lookup handler rows -> <read>.absent.

    Such a path appears only in run reports, which this converter does not
    rewrite and no golden pins (the table's reason); a negative contract
    reaches an intermediate's `no_match` through the rule above."""
    return segments


def rule_study_day(table, spec, segments, cleared):
    """study_day.date -> study_day.source (P-OP-1)."""
    return _rename_after(
        segments, "study_day", "date", table.new_name("spec.study_day.date")
    )


def rule_baseline(table, spec, segments, cleared):
    """baseline_flag.reference_date and baseline_flag.value -> reference and source."""
    out = _rename_after(
        segments,
        "baseline_flag",
        "reference_date",
        table.new_name("spec.baseline_flag.reference_date"),
    )
    return _rename_after(
        out, "baseline_flag", "value", table.new_name("spec.baseline_flag.value")
    )


def rule_bounds(table, spec, segments, cleared):
    """diff.bounds -> diff.closed (P-ORD-2)."""
    return _rename_after(segments, "diff", "bounds", table.new_name("spec.diff.bounds"))


def rule_derive(table, spec, segments, cleared):
    """aggregate.derive[N] -> aggregate.derivations.<name> (P-BIND-1)."""
    for index in range(len(segments) - 2):
        if segments[index : index + 2] == ["aggregate", "derive"] and segments[
            index + 2
        ].startswith("["):
            bindings = _navigate(spec, segments[: index + 2])
            position = int(segments[index + 2][1:-1])
            if not isinstance(bindings, list) or position >= len(bindings):
                return None
            name = (
                bindings[position].get("name")
                if isinstance(bindings[position], dict)
                else None
            )
            if name is None:
                return None
            return (
                segments[: index + 1]
                + ["derivations", path_segment(str(name))]
                + segments[index + 3 :]
            )
    return segments


def rule_datasets(table, spec, segments, cleared):
    """<section>.datasets -> <section>.transport (P-PRJ-1)."""
    if len(segments) >= 2 and segments[0] in SECTIONS and segments[1] == "datasets":
        return [
            segments[0],
            table.new_name("project.submission_section_class.datasets"),
        ] + segments[2:]
    return segments


def rule_standard(table, spec, segments, cleared):
    """<section>.standard -> <section>.standards[0] (D16)."""
    if len(segments) >= 2 and segments[0] in SECTIONS and segments[1] == "standard":
        return [segments[0], "standards", "[0]"] + segments[2:]
    return segments


def rule_origin(table, spec, segments, cleared):
    """submission.origin -> submission.origins[0] (P-ORIGIN-1), column or value level."""
    out = []
    for index, segment in enumerate(segments):
        previous = segments[index - 1] if index else ""
        if segment == "origin" and (
            previous == "submission" or previous.startswith("[")
        ):
            out.extend(["origins", "[0]"])
        else:
            out.append(segment)
    return out


def rule_cleared(table, spec, segments, cleared):
    """the cleared path, for a finding about a child-layer null -> clear[N] (P-CLR-1)."""
    dotted = joined(segments)
    if dotted in cleared:
        return ["clear", f"[{cleared.index(dotted)}]"]
    return segments


RULES = {
    "intermediates[N] and rows[N]": rule_indexed,
    "derivation.compute.expr and derivation.compute": rule_compute,
    "<field>.literal": rule_literal,
    "intermediates[N].no_match": rule_intermediate_no_match,
    "<read>.no_match in a run report's lookup handler rows": rule_read_no_match,
    "study_day.date": rule_study_day,
    "baseline_flag.reference_date and baseline_flag.value": rule_baseline,
    "diff.bounds": rule_bounds,
    "aggregate.derive[N]": rule_derive,
    "<section>.datasets": rule_datasets,
    "<section>.standard": rule_standard,
    "submission.origin": rule_origin,
    "the cleared path, for a finding about a child-layer null": rule_cleared,
}

# A `no_match` under an indexed intermediate is renamed, and a derive
# binding named, while the index still finds them; a cleared path is
# compared once every other segment is in 1.0 spelling.
ORDER = (
    "intermediates[N].no_match",
    "aggregate.derive[N]",
    "intermediates[N] and rows[N]",
    "derivation.compute.expr and derivation.compute",
    "<field>.literal",
    "<read>.no_match in a run report's lookup handler rows",
    "study_day.date",
    "baseline_flag.reference_date and baseline_flag.value",
    "diff.bounds",
    "<section>.datasets",
    "<section>.standard",
    "submission.origin",
    "the cleared path, for a finding about a child-layer null",
)


def rewrite_path(path, spec, cleared, table):
    """One diagnostic spec path in 1.0 notation; None when a member cannot be named."""
    segments = segments_of(path)
    for name in ORDER:
        table.path_rule(name)
        segments = RULES[name](table, spec, segments, cleared)
        if segments is None:
            return None
    return joined(segments)


def condition_change(entry, benchmark):
    """('rename', new name), ('manual', reason) or None for a negative's condition."""
    listed = [
        item
        for item in entry.get("negative_benchmarks") or []
        if item.split(" ", 1)[0] == benchmark
    ]
    if listed and not all("path only" in item for item in listed):
        return (
            "manual",
            f"{listed[0]}; re-author expected/error.yaml independently (D60, D61)",
        )
    kind = entry.get("kind")
    if kind == "rename" and entry.get("proposed"):
        return "rename", entry["proposed"]
    if kind == "removed":
        return (
            "manual",
            f"condition {entry['today']} is retired ({entry.get('ref')}); retire or re-author this benchmark",
        )
    return None
