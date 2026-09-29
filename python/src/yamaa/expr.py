"""Expression registry evaluation (operations/expressions). One function per
expression family."""

import calendar
import copy
import re
from datetime import date as _date

from . import numeric as _numeric
from . import pred as _pred
from .errors import YamaaError
from .values import (
    YDate,
    YDateTime,
    ascii_fold,
    ascii_lower,
    ascii_upper,
    comparable,
    compare,
    date_prefix_parts,
    date_text,
    datetime_text,
    float_text,
    is_missing,
    normalize_number,
    parse_date,
    parse_datetime,
)

_ABSENT = object()  # lookup selected nothing

# REQ-0358/REQ-0359: the handled_expression_class fields.
HANDLED_FIELDS = {"value", "unconvertible"}


def derivation_requirement(d):
    """REQ-0286 for a mapping naming no single registered keyword, REQ-0320
    for a derivation that is not a mapping or string at all."""
    return "REQ-0286" if isinstance(d, dict) else "REQ-0320"


def _fail(where, phase, condition, requirement, context=None):
    raise YamaaError(
        phase=phase,
        condition=condition,
        requirement=requirement,
        spec_paths=[where],
        context=context or {},
    )


def eval_expr(node, ctx):
    """node: a derivation mapping (one registry key) or {'value':...} handled form.

    While an expression evaluates, `ctx.where` is the path of the node being
    evaluated: `<derivation>.<operation>`, extended into nested expressions.
    A failure an operation raises therefore names the operation, or a field
    under it, never only the derivation that holds it."""
    if isinstance(node, dict) and "value" in node and set(node) <= HANDLED_FIELDS:
        return eval_at(node["value"], ctx, f"{ctx.where}.value")
    if not isinstance(node, dict) or len(node) != 1:
        _fail(
            ctx.where,
            "validation",
            "invalid_field_type",
            derivation_requirement(node),
            {"derivation": node},
        )
    key = next(iter(node))
    fn = _REGISTRY.get(key)
    if fn is None:
        _fail(ctx.where, "validation", "unknown_field", "REQ-0321", {"expression": key})
    outer = ctx.where
    ctx.where = f"{outer}.{key}"
    try:
        return fn(node[key], ctx)
    finally:
        ctx.where = outer


def eval_at(node, ctx, where):
    """Evaluate a nested expression written at `where`."""
    outer = ctx.where
    ctx.where = where
    try:
        return eval_expr(node, ctx)
    finally:
        ctx.where = outer


def keys_of(e, row):
    """The output keys of `row` as reported values, or None while a key is
    not yet derived: a partial key would name no row."""
    if row is None or any(k not in row for k in e.keys):
        return None
    return {k: json_value(row[k]) for k in e.keys}


def json_value(v):
    """A runtime value as a failure reports it: temporal values as text."""
    if isinstance(v, YDateTime):
        return datetime_text(v)
    if isinstance(v, YDate):
        return date_text(v)
    return v


def _source_var_payload(payload):
    """Normalize the `source` expression payload to (variable, filter,
    no-record answer, selection). REQ-0111: a structured source keeps its
    `order_by`/`keep`. A dataset read that reaches no record answers missing
    (REQ-0111/REQ-0355): its variable still exists in context, so `absent`,
    which answers only a variable absent from context (REQ-0345), never
    applies to it."""
    if isinstance(payload, str):
        return payload, None, _ABSENT, None
    if not isinstance(payload, dict):
        _fail("<source>", "validation", "invalid_field_type", "REQ-1051", {})
    sel = None
    if "order_by" in payload or "keep" in payload:
        sel = {"order_by": payload.get("order_by"), "keep": payload.get("keep")}
    return (
        payload.get("variable"),
        payload.get("filter"),
        _ABSENT,
        sel,
    )


def _filtered_source_payload(payload):
    if isinstance(payload, str):
        return payload, None
    return payload.get("variable"), payload.get("filter")


def _distinct_key(v):
    if isinstance(v, bool):
        return ("bool", v)
    if isinstance(v, (int, float)):
        return ("num", float(v))
    if isinstance(v, str):
        return ("str", v)
    return ("repr", repr(v))


def _reads_origin(ctx, var):
    """True when a column-phase read names the dataset the row was built
    from: its records are the ones the key combination was derived from."""
    if not getattr(ctx, "_col_phase", False):
        return False
    return var.split(".")[0] == ctx.e._origins[ctx.i]


def _one_record(var, filt, missing, sel, ctx, site=None):
    """Resolve a qualified source to one value. The filter narrows the
    records (REQ-0355) and `order_by`/`keep` chooses among the survivors
    (REQ-0354). Without a selection, the records a key combination was
    derived from must agree on one value (REQ-0075), and more than one
    joined record is an unhandled multiple match (REQ-0127)."""
    site = site or ctx.where
    recs = ctx.source_records(var)
    if filt is not None:
        recs = [
            r for r in recs if ctx.record_predicate(filt, r, var.split(".")[0]) is True
        ]
        if not recs:
            return None  # REQ-0355: empty filtered result is an absent match
    value_field = ctx.value_field(var)
    if not recs:
        if missing is not _ABSENT:
            return missing  # REQ-0129 / REQ-0345
        # REQ-0124: lookup with no_match declared returns its literal.
        ds = var.split(".")[0] if "." in var else None
        if ds is not None:
            decl = ctx.e.lookups_decl.get(ds)
            if decl is not None and "no_match" in decl:
                return decl["no_match"]
        return None
    if len(recs) > 1:
        if sel is not None:
            recs = _choose(recs, sel, ctx)
        elif _reads_origin(ctx, var):
            present = [
                r.get(value_field) for r in recs if not is_missing(r.get(value_field))
            ]
            seen_h, distinct = set(), []
            for v in present:
                h = _distinct_key(v)
                if h not in seen_h:
                    seen_h.add(h)
                    distinct.append(v)
            if len(distinct) > 1:
                e = ctx.e
                _fail(
                    site,
                    "derivation",
                    "multiple_values_per_key",
                    "REQ-0075",
                    {
                        "identifier": var,
                        "value_count": len(distinct),
                        "keys": [{k: e.rows[ctx.i].get(k) for k in e.keys}],
                    },
                )
            return distinct[0] if distinct else None
        else:
            _fail(
                site,
                "join",
                "multiple_matches",
                "REQ-0127",
                _multiple_match_context(var, recs, ctx),
            )
    rec = recs[0]
    if value_field not in rec:
        _fail(ctx.where, "validation", "unknown_field", "REQ-0103", {"identifier": var})
    return rec[value_field]


def _multiple_match_context(var, recs, ctx):
    """REQ-0143: report the implicit join's match the way a named lookup
    reports its own."""
    e = ctx.e
    ds = var.split(".")[0]
    table = e.inputs.get(ds)
    key = [k for k in e.keys if table is not None and k in table.fields]
    out = {
        "intermediate": f"intermediate({ds})",
        "dataset": ds,
        "key": key,
        "intermediate_key": {k: recs[0].get(k) for k in key},
        "match_count": len(recs),
    }
    if getattr(ctx, "_col_phase", False):
        out["keys"] = [{k: e.rows[ctx.i].get(k) for k in e.keys}]
    return out


def _choose(recs, sel, ctx):
    """REQ-0354: order the survivors and retain `first` or `last`; ties
    keep record order (a stable sort)."""
    ordered = ctx.order_records(recs, sel["order_by"])
    return [ordered[0] if sel["keep"] == "first" else ordered[-1]]


def ev_source(payload, ctx):
    var, filt, missing, sel = _source_var_payload(payload)
    if "." in var:
        return _one_record(var, filt, missing, sel, ctx)
    return ctx.value(var)


def ev_literal(payload, ctx):
    return normalize_number(payload) if isinstance(payload, float) else payload


def ev_first_available(payload, ctx):
    for s in payload["sources"]:
        var, filt = _filtered_source_payload(s)
        v = _one_record(var, filt, _ABSENT, None, ctx) if "." in var else ctx.value(var)
        if not is_missing(v):
            return v
    return payload.get("missing")


def ev_greatest_least(payload, ctx, which):
    best = None
    for var in payload["sources"]:
        v = ctx.value(var)
        if is_missing(v):
            continue
        if best is not None and not comparable(best, v):
            _fail(
                ctx.where,
                "validation",
                "incomparable_sources",
                "REQ-0324",
                {
                    "sources": payload["sources"],
                    "types": [_runtime_type_name(best), _runtime_type_name(v)],
                },
            )
        if best is None or (compare(v, best) > 0) == (which == "greatest"):
            best = v
    return best


def ev_flag(payload, ctx):
    if isinstance(payload, str):
        cond_text, site = payload, ctx.where
        tv, fv, mv = "Y", _ABSENT, _ABSENT
    elif isinstance(payload, dict):
        cond = payload.get("condition")
        if not isinstance(cond, str):
            _fail(
                ctx.where,
                "validation",
                "invalid_field_type",
                "REQ-1257",
                {"expected": "predicate", "field": "condition"},
            )
        cond_text, site = cond, ctx.where + ".condition"
        tv = payload.get("true_value", "Y")
        fv = payload.get("false_value", _ABSENT)
        mv = payload.get("missing", _ABSENT)
    else:
        _fail(
            ctx.where,
            "validation",
            "invalid_field_type",
            "REQ-1257",
            {"expected": "predicate string or mapping"},
        )
    node, _ = _pred.parse(cond_text, site)
    try:
        holds = _pred.evaluate(node, ctx.value, site)
    except YamaaError as e:
        if e.condition == "unknown_field":
            raise YamaaError(
                phase=e.phase,
                condition=e.condition,
                requirement="REQ-0189",
                spec_paths=[site],
                context={"identifier": e.context.get("identifier")},
            ) from e
        raise
    if holds is True:
        return tv
    if holds is False:
        return None if fv is _ABSENT else fv
    return None if mv is _ABSENT else mv


def _raw_operand(var, ctx):
    """Resolve an operand without handler substitution or type check."""
    if "." not in var:
        return ctx._match_operand(var)
    return _one_record(var, None, _ABSENT, None, ctx)


def _incompatible(ctx, field, requirement, var, expected, v):
    """REQ-0323: an operand of a type the operation does not take. The static
    check catches every declared type; a value with no static type (an
    intermediate's column) reports the same validation condition when it
    arrives, at the operand's field."""
    _fail(
        f"{ctx.where}.{field}" if field else ctx.where,
        "validation",
        "incompatible_input_type",
        requirement,
        {"source": var, "expected": expected, "actual": _runtime_type_name(v)},
    )


def check_pad_width(width, where):
    """REQ-1261: `width` is a positive integer. A value that is not an
    integer fails its type (REQ-0287); one below one fails as REQ-1261
    says."""
    if isinstance(width, bool) or not isinstance(width, int):
        _fail(
            where,
            "validation",
            "invalid_field_type",
            "REQ-0287",
            {"expected": "int", "actual": _runtime_type_name(width)},
        )
    if width < 1:
        _fail(
            where,
            "validation",
            "invalid_field_type",
            "REQ-1261",
            {"expected": "positive int", "actual": width},
        )


def group_out_of_range(where, group, group_count, pattern):
    """REQ-0816: a `str_extract.group` the pattern does not capture."""
    _fail(
        where,
        "validation",
        "regex_group_out_of_range",
        "REQ-0816",
        {"group": group, "group_count": group_count, "pattern": pattern},
    )


# REQ-0010: the runtime types with a canonical text; a Boolean has none.
_TEXT_SOURCES = "str, int, float, date, or datetime"


def _canonical_text(v, var, ctx):
    """REQ-0010 scalar -> canonical text. Booleans fail conversion."""
    if isinstance(v, bool):
        _incompatible(ctx, "source", "REQ-0010", var, _TEXT_SOURCES, v)
    if isinstance(v, str):
        return v
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return float_text(v)
    if isinstance(v, YDateTime):
        return datetime_text(v)
    if isinstance(v, YDate):
        return date_text(v)
    _incompatible(ctx, "source", "REQ-0010", var, _TEXT_SOURCES, v)


def ev_str_pad(payload, ctx):
    if not isinstance(payload, dict):
        _fail(
            ctx.where,
            "validation",
            "invalid_field_type",
            "REQ-1261",
            {"expected": "mapping"},
        )
    check_pad_width(payload.get("width"), ctx.where + ".width")
    width = payload["width"]
    var = payload.get("source")
    v = _raw_operand(var, ctx)
    if is_missing(v):
        return payload.get("missing")
    t = _canonical_text(v, var, ctx)
    return t if len(t) >= width else " " * (width - len(t)) + t


def ev_case(payload, ctx):
    for n, item in enumerate(payload):
        if not isinstance(item, dict):
            _fail(ctx.where, "validation", "invalid_field_type", "REQ-0339", {})
        if "when" in item:
            site = f"{ctx.where}[{n}].when"
            site_ctx = copy.copy(ctx)
            site_ctx.where = site
            node, _ = _pred.parse(item["when"], site)
            try:
                holds = _pred.evaluate(node, site_ctx.value, site)
            except YamaaError as e:
                if e.condition == "unknown_field":
                    raise YamaaError(
                        phase=e.phase,
                        condition=e.condition,
                        requirement="REQ-0189",
                        spec_paths=e.spec_paths,
                        context={"identifier": e.context.get("identifier")},
                    ) from e
                raise
            if holds is True:
                then = item["then"]
                if isinstance(then, str):
                    then = {"source": then}
                return eval_at(then, ctx, f"{ctx.where}[{n}].then")
        elif "otherwise" in item and len(item) == 1:
            other = item["otherwise"]
            if isinstance(other, str):
                other = {"source": other}
            return eval_at(other, ctx, f"{ctx.where}[{n}].otherwise")
        else:
            _fail(ctx.where, "validation", "invalid_field_type", "REQ-0339", {})
    return None


def ev_mapping(payload, ctx):
    """REQ-1110: look a string source up in the dictionary. `missing` answers
    a missing source and `unmapped` a present one with no entry; without
    its handler, each condition fails (REQ-0344)."""
    site = ctx.where
    var, filt = _filtered_source_payload(payload["source"])
    v = (
        _one_record(var, filt, _ABSENT, None, ctx, site=site)
        if "." in var
        else ctx.value(var)
    )
    if is_missing(v):
        if "missing" in payload:
            return payload["missing"]
        _fail(
            site,
            "mapping",
            "missing_input",
            "REQ-0334",
            {"variable": var},
        )
    if not isinstance(v, str):
        _incompatible(ctx, "source", "REQ-0304", var, "str", v)
    # REQ-1110: dict is inline or a YAML path loaded via project resources.
    d = payload["dict"]
    if isinstance(d, str):
        d = ctx.e._load_dict_yaml(d, f"{site}.dict")
    case_sensitive = payload.get("case_sensitive", True)
    if case_sensitive:
        key = v
    else:
        folded = {}
        for k in d:
            folded.setdefault(ascii_fold(k), []).append(k)
        collisions = sorted(fk for fk, entries in folded.items() if len(entries) > 1)
        if collisions:
            _fail(
                f"{site}.dict",
                "validation",
                "ambiguous_dictionary",
                "REQ-0714",
                {"folded_key": collisions[0], "entries": folded[collisions[0]]},
            )
        key = ascii_fold(v)
        d = {ascii_fold(k): val for k, val in d.items()}
    if key in d:
        return d[key]
    if "unmapped" in payload:
        return payload["unmapped"]
    # `missing` never answers a present source with no entry.
    _fail(
        site,
        "mapping",
        "unmapped_value",
        "REQ-0334",
        {"source": var, "value": v},
    )


def ev_cut(payload, ctx):
    src = payload["source"]
    v = ctx.value(src) if "." not in src else _one_record(src, None, _ABSENT, None, ctx)
    if is_missing(v):
        if "missing" in payload:
            return payload["missing"]
        _fail(
            ctx.where,
            "cut",
            "missing_input",
            "REQ-0334",
            {"variable": src.split(".")[-1]},
        )
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        # REQ-0306: validation catches every statically typed source; a
        # source with no static type (an intermediate's column) reports the
        # same validation condition when its value arrives.
        _fail(
            f"{ctx.where}.source",
            "validation",
            "incompatible_input_type",
            "REQ-0306",
            {"source": src, "expected": "numeric", "actual": type(v).__name__},
        )
    breaks = payload["breaks"]
    labels = payload["labels"]
    right = payload.get("right", False)
    x = float(v)
    n = len(breaks)
    if not right:
        if x < breaks[0]:
            return labels[0]
        for i in range(n - 1):
            if breaks[i] <= x < breaks[i + 1]:
                return labels[i + 1]
        return labels[-1]
    if x <= breaks[0]:
        return labels[0]
    for i in range(n - 1):
        if breaks[i] < x <= breaks[i + 1]:
            return labels[i + 1]
    return labels[-1]


def ev_round_half_away_from_zero(payload, ctx):
    """REQ-0418/REQ-1172: SAS-style rounding, ties half away from zero."""
    src = payload["source"]
    v = ctx.value(src) if "." not in src else _one_record(src, None, _ABSENT, None, ctx)
    if is_missing(v):
        return None
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        _incompatible(ctx, "source", "REQ-0418", src, "numeric", v)
    return _numeric.round_half_away_from_zero(v, payload["digits"])


def _str_operand(var, ctx):
    """Resolve a string operand WITHOUT handler substitution."""
    v = ctx.value(var) if "." not in var else _one_record(var, None, _ABSENT, None, ctx)
    if not is_missing(v) and not isinstance(v, str):
        _incompatible(ctx, "source", "REQ-0308", var, "str", v)
    return v


def ev_str_extract(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")  # None when unhandled -> missing result
    try:
        rx = re.compile(payload["pattern"])
    except re.error:
        _fail(
            ctx.where + ".pattern",
            "validation",
            "invalid_regex",
            "REQ-0827",
            {"pattern": payload["pattern"]},
        )
    m = rx.search(v)
    if not m:
        return payload.get("no_match")  # fatal when omitted (REQ-0344)
    g = payload.get("group", 0)
    if g > rx.groups:
        # REQ-0816: validation rejects the group before any row reads it.
        group_out_of_range(ctx.where + ".group", g, rx.groups, payload["pattern"])
    return m.group(g)


def ev_str_concat(payload, ctx):
    parts = []
    for j, s in enumerate(payload["sources"]):
        v = eval_at(s, ctx, f"{ctx.where}.sources[{j}]")
        if is_missing(v):
            return payload.get("missing")  # fatal when omitted
        if not isinstance(v, str):
            _incompatible(ctx, f"sources[{j}]", "REQ-0308", None, "str", v)
        parts.append(v)
    return "".join(parts)


_TEMPLATE_RE = re.compile(r"{{|}}|{[A-Za-z_][A-Za-z0-9_.]*}")
_PLACEHOLDER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*")


def parse_template(text, where):
    """REQ-0453/REQ-0461: split a template into ("text", literal) and
    ("name", variable) parts. A brace pair is one literal brace; a lone
    brace or a placeholder that is not a variable name fails validation,
    reporting the reason and the placeholder."""
    parts, literal, i = [], [], 0

    def bad(reason, placeholder=None):
        ctx = {"reason": reason}
        if placeholder is not None:
            ctx["placeholder"] = placeholder
        _fail(where, "validation", "invalid_string_template", "REQ-0461", ctx)

    while i < len(text):
        if text.startswith("{{", i) or text.startswith("}}", i):
            literal.append(text[i])
            i += 2
            continue
        c = text[i]
        if c == "}":
            bad("unmatched_brace")
        if c != "{":
            literal.append(c)
            i += 1
            continue
        end = text.find("}", i + 1)
        if end < 0:
            bad("unmatched_brace")
        name = text[i + 1 : end]
        if "{" in name or not _PLACEHOLDER.fullmatch(name):
            bad("invalid_placeholder", name)
        if literal:
            parts.append(("text", "".join(literal)))
            literal = []
        parts.append(("name", name))
        i = end + 1
    if literal:
        parts.append(("text", "".join(literal)))
    return parts


def ev_str_template(payload, ctx):
    template = payload if isinstance(payload, str) else payload["template"]
    missing = None if isinstance(payload, str) else payload.get("missing")
    out = []
    for kind, part in parse_template(template, ctx.where):
        if kind == "text":
            out.append(part)
            continue
        v = (
            ctx.value(part)
            if "." not in part
            else _one_record(part, None, _ABSENT, None, ctx)
        )
        if is_missing(v):
            return missing  # fatal when omitted (REQ-0344)
        if not isinstance(v, str):
            field = "template" if isinstance(payload, dict) else None
            _incompatible(ctx, field, "REQ-0458", part, "str", v)
        out.append(v)
    return "".join(out)


def ev_str_case(payload, ctx):
    """REQ-1114: change ASCII letter case per `to` (upper/lower/sentence/title)."""
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")
    to = payload.get("to")
    if to == "upper":
        return ascii_upper(v)
    if to == "lower":
        return ascii_lower(v)
    if to == "sentence":
        if not v:
            return v
        return ascii_upper(v[0]) + ascii_lower(v[1:])
    if to == "title":
        out = []
        i, n = 0, len(v)
        while i < n:
            c = v[i]
            if "A" <= c <= "Z" or "a" <= c <= "z":
                j = i + 1
                while j < n and ("A" <= v[j] <= "Z" or "a" <= v[j] <= "z"):
                    j += 1
                run = v[i:j]
                out.append(ascii_upper(run[0]) + ascii_lower(run[1:]))
                i = j
            else:
                out.append(c)
                i += 1
        return "".join(out)
    _fail(
        ctx.where + ".to",
        "validation",
        "value_not_permitted",
        "REQ-0287",
        {"value": to, "permitted": ["upper", "lower", "sentence", "title"]},
    )


def ev_str_contains(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")
    try:
        rx = _pred.normalize_pattern(payload["pattern"])
    except re.error:
        _fail(
            ctx.where + ".pattern",
            "validation",
            "invalid_regex",
            "REQ-0827",
            {"pattern": payload["pattern"]},
        )
    return rx.search(v) is not None


def ev_compute(payload, ctx):
    node, _ = _numeric.parse(payload["expr"], ctx.where + ".expr")
    return _numeric.evaluate(node, ctx.compute_ident, ctx.where, payload["expr"])


def _date_operand(var, ctx):
    """Resolve a date operand WITHOUT handler substitution (cf. _str_operand)."""
    return (
        ctx.value(var) if "." not in var else _one_record(var, None, _ABSENT, None, ctx)
    )


def ev_date_diff(payload, ctx):
    s = (
        ctx.value(payload["start"])
        if "." not in payload["start"]
        else _one_record(payload["start"], None, _ABSENT, None, ctx)
    )
    e = (
        ctx.value(payload["end"])
        if "." not in payload["end"]
        else _one_record(payload["end"], None, _ABSENT, None, ctx)
    )
    check_date_diff_bounds(payload, ctx.where)
    if is_missing(s) or is_missing(e):
        return None
    if not isinstance(s, (YDate, YDateTime)):
        _incompatible(ctx, "start", "REQ-0004", payload["start"], "date", s)
    if type(s) is not type(e):
        _incompatible(ctx, "end", "REQ-0004", payload["end"], _runtime_type_name(s), e)
    unit = payload["unit"]
    bounds = payload.get("bounds", "exclusive")
    if unit == "day":
        days = (
            (e - s).days
            if isinstance(s, YDate)
            else int((e - s).total_seconds() // 86400)
        )
        if bounds == "exclusive":
            return days
        if bounds == "inclusive":
            return days + (1 if days >= 0 else -1)
        return days - (1 if days >= 0 else -1)  # between
    if unit == "week":
        days = (e - s).days
        q = abs(days) // 7
        return q if days >= 0 else -q
    if unit == "month":
        return _whole_months(s, e)
    if unit == "year":
        m = _whole_months(s, e)
        q = abs(m) // 12
        return q if m >= 0 else -q
    _fail(
        ctx.where + ".unit",
        "validation",
        "value_not_permitted",
        "REQ-0287",
        {"value": unit, "permitted": ["day", "week", "month", "year"]},
    )


def check_date_diff_bounds(payload, where):
    """REQ-0613: a unit other than `day` counts whole units, so its only
    permitted `bounds` is `exclusive`."""
    bounds = payload.get("bounds", "exclusive")
    if payload.get("unit") != "day" and bounds != "exclusive":
        _fail(
            where + ".bounds",
            "validation",
            "value_not_permitted",
            "REQ-0613",
            {"value": bounds, "permitted": ["exclusive"]},
        )


def _whole_months(s, e):
    """REQ-0595: count monthly anniversaries of s on or before e, with the
    anniversary day clamped to the month length."""
    import calendar

    sign = 1
    if e < s:
        s, e = e, s
        sign = -1
    count = 0
    y, m = s.year, s.month
    while True:
        m += 1
        if m > 12:
            m, y = 1, y + 1
        d = min(s.day, calendar.monthrange(y, m)[1])
        if YDate(y, m, d) <= e:
            count += 1
        else:
            break
    return sign * count


def _invalid_date_text(payload, ctx, src):
    """REQ-0588: present text that is neither a complete date nor a date
    prefix answers `invalid`, or fails at the operation naming its source."""
    if "invalid" not in payload:
        _fail(
            ctx.where,
            "impute",
            "invalid_date_text",
            "REQ-0588",
            {"source": payload["source"], "value": src},
        )
    return payload["invalid"]


def _completed_date(payload, ctx, src, year, month, day):
    """REQ-0608: the completed value must be a real calendar date."""
    try:
        return YDate(year, month, day)
    except ValueError:
        if "invalid" not in payload:
            _fail(
                ctx.where,
                "impute",
                "invalid_calendar_date",
                "REQ-0608",
                {"value": src, "completed": f"{year:04d}-{month:02d}-{day:02d}"},
            )
        return None


def ev_date_impute(payload, ctx):
    src = _date_operand(payload["source"], ctx)
    if is_missing(src):
        return payload.get("missing")
    if isinstance(src, YDate):
        completed, precision = src, src.precision
    elif isinstance(src, str):
        parts = date_prefix_parts(src)
        if parts is None:
            return _invalid_date_text(payload, ctx, src)
        year, month, day = parts
        if day is not None:
            completed = _completed_date(payload, ctx, src, year, month, day)
            if completed is None:
                return payload["invalid"]
            precision = "D"
        else:
            if month is None:
                if payload.get("minimum_source_precision", "year") == "month":
                    return None
                month = payload["month"]
            d = payload["day"]
            if d == "first":
                d = 1
            elif d == "last":
                d = calendar.monthrange(year, month)[1]
            completed = _completed_date(payload, ctx, src, year, month, d)
            if completed is None:
                return payload["invalid"]
            precision = "M" if parts[1] is not None else "Y"
    else:
        requirement = "REQ-0606" if isinstance(src, YDateTime) else "REQ-0323"
        _incompatible(ctx, "source", requirement, payload["source"], "date or str", src)
    nb = payload.get("not_before")
    if nb is not None:
        bound = (
            ctx.value(nb)
            if "." not in nb
            else _one_record(nb, None, _ABSENT, None, ctx)
        )
        if not is_missing(bound):
            if type(bound) is not YDate:
                _incompatible(ctx, "not_before", "REQ-0337", nb, "date", bound)
            if precision != "D" and completed < bound:
                lo = (
                    YDate(completed.year, completed.month, 1)
                    if precision == "M"
                    else YDate(completed.year, 1, 1)
                )
                hi = (
                    YDate(
                        completed.year,
                        completed.month,
                        calendar.monthrange(completed.year, completed.month)[1],
                    )
                    if precision == "M"
                    else YDate(completed.year, 12, 31)
                )
                if lo <= bound <= hi:
                    completed = bound
                else:
                    return None
    out = YDate(completed.year, completed.month, completed.day, precision=precision)
    return out


def ev_date_precision(payload, ctx):
    src = _date_operand(payload["source"], ctx)
    if is_missing(src):
        return payload.get("missing")
    if isinstance(src, YDate):
        return src.precision
    if isinstance(src, str):
        parts = date_prefix_parts(src)
        if parts is None:
            return _invalid_date_text(payload, ctx, src)
        _, month, day = parts
        return "D" if day is not None else ("M" if month is not None else "Y")
    requirement = "REQ-0606" if isinstance(src, YDateTime) else "REQ-0337"
    _incompatible(ctx, "source", requirement, payload["source"], "date or str", src)


def ev_to_date(payload, ctx):
    v = (
        ctx.value(payload["source"])
        if "." not in payload["source"]
        else _one_record(payload["source"], None, _ABSENT, None, ctx)
    )
    if is_missing(v):
        return None
    if isinstance(v, YDateTime):
        return YDate(v.year, v.month, v.day)
    if isinstance(v, str):
        # REQ-0607: ISO date text parses directly; ISO datetime text keeps
        # its calendar date. Its conditions are the temporal ones REQ-0348
        # puts on the `impute` stage.
        try:
            return parse_date(v)
        except ValueError:
            pass
        try:
            m = parse_datetime(v)
        except ValueError:
            _fail(
                ctx.where,
                "impute",
                "invalid_date_text",
                "REQ-0607",
                {"source": payload["source"], "value": v},
            )
        return YDate(m.year, m.month, m.day)
    to_date_incompatible(ctx.where, _runtime_type_name(v))


def to_date_incompatible(where, actual):
    """REQ-0607: `to_date` takes a datetime or ISO text, never a date; the
    failure names the operation and its `source` field."""
    _fail(
        where,
        "validation",
        "incompatible_input_type",
        "REQ-0607",
        {
            "operation": "to_date",
            "source": "source",
            "expected": "datetime, ISO date text, or ISO 8601 datetime text",
            "actual": actual,
        },
    )


def ev_study_day(payload, ctx):
    d = (
        ctx.value(payload["date"])
        if "." not in payload["date"]
        else _one_record(payload["date"], None, _ABSENT, None, ctx)
    )
    r = (
        ctx.value(payload["reference"])
        if "." not in payload["reference"]
        else _one_record(payload["reference"], None, _ABSENT, None, ctx)
    )
    if is_missing(d) or is_missing(r):
        return None
    if not isinstance(d, (YDate, YDateTime)):
        _incompatible(ctx, "date", "REQ-0004", payload["date"], "date", d)
    if type(d) is not type(r):
        expected = _runtime_type_name(d)
        _incompatible(ctx, "reference", "REQ-0004", payload["reference"], expected, r)
    delta = (d - r).days
    return delta + 1 if delta >= 0 else delta


_EPOCH_ORDINAL = _date(1970, 1, 1).toordinal()


def _runtime_type_name(v):
    if v is None:
        return "null"
    if isinstance(v, dict):
        return "mapping"
    if isinstance(v, list):
        return "list"
    if isinstance(v, bool):
        return "bool"
    if isinstance(v, int):
        return "int"
    if isinstance(v, float):
        return "float"
    if isinstance(v, YDateTime):
        return "datetime"
    if isinstance(v, YDate):
        return "date"
    if isinstance(v, str):
        return "str"
    return type(v).__name__


def _collected_datetime_precision(text):
    """'S' if the text supplied a time of day, 'D' if a complete date,
    None if the text is not a complete date or datetime."""
    try:
        parse_datetime(text)
        return "S"
    except ValueError:
        pass
    try:
        parse_date(text)
        return "D"
    except ValueError:
        pass
    return None


def ev_datetime_impute(payload, ctx):
    time_rule = payload.get("time")
    if time_rule not in ("first", "last"):
        _fail(
            ctx.where,
            "validation",
            "value_not_permitted",
            "REQ-1184",
            {"field": "time", "value": str(time_rule), "permitted": ["first", "last"]},
        )
    var = payload["source"]
    v = ctx.value(var) if "." not in var else _one_record(var, None, _ABSENT, None, ctx)
    if is_missing(v):
        if "missing" in payload:
            return payload["missing"]
        _fail(
            ctx.where,
            "impute",
            "missing_input",
            "REQ-1182",
            {"operation": "datetime_impute", "source": var},
        )
    if isinstance(v, YDateTime):
        return v
    if not isinstance(v, str):
        _fail(
            ctx.where,
            "validation",
            "incompatible_input_type",
            "REQ-1182",
            {
                "operation": "datetime_impute",
                "source": "source",
                "expected": "str",
                "actual": _runtime_type_name(v),
            },
        )
    precision = _collected_datetime_precision(v)
    if precision is None:
        if "invalid" in payload:
            return payload["invalid"]
        _fail(
            ctx.where,
            "impute",
            "invalid_datetime_text",
            "REQ-1182",
            {
                "operation": "datetime_impute",
                "source": var,
                "value": v,
            },
        )
    if precision == "S":
        return parse_datetime(v)
    d = parse_date(v)
    if time_rule == "first":
        return YDateTime(d.year, d.month, d.day, 0, 0, 0, collected_precision="day")
    return YDateTime(d.year, d.month, d.day, 23, 59, 59, collected_precision="day")


def ev_datetime_precision(payload, ctx):
    var = payload["source"]
    v = ctx.value(var) if "." not in var else _one_record(var, None, _ABSENT, None, ctx)
    if is_missing(v):
        if "missing" in payload:
            return payload["missing"]
        _fail(
            ctx.where,
            "impute",
            "missing_input",
            "REQ-1183",
            {"operation": "datetime_precision", "source": var},
        )
    if isinstance(v, YDateTime):
        return "S" if v.collected_precision == "second" else "D"
    if not isinstance(v, str):
        _fail(
            ctx.where,
            "validation",
            "incompatible_input_type",
            "REQ-1183",
            {
                "operation": "datetime_precision",
                "source": "source",
                "expected": "str or datetime",
                "actual": _runtime_type_name(v),
            },
        )
    precision = _collected_datetime_precision(v)
    if precision is None:
        if "invalid" in payload:
            return payload["invalid"]
        _fail(
            ctx.where,
            "impute",
            "invalid_datetime_text",
            "REQ-1183",
            {
                "operation": "datetime_precision",
                "source": var,
                "value": v,
            },
        )
    return precision


def ev_to_epoch_day(payload, ctx):
    var = payload["source"]
    v = ctx.value(var) if "." not in var else _one_record(var, None, _ABSENT, None, ctx)
    if is_missing(v):
        return None
    if isinstance(v, YDateTime) or not isinstance(v, YDate):
        _fail(
            ctx.where,
            "validation",
            "incompatible_input_type",
            "REQ-0606",
            {
                "operation": "to_epoch_day",
                "source": "source",
                "expected": "date",
                "actual": _runtime_type_name(v),
            },
        )
    return v.toordinal() - _EPOCH_ORDINAL


def ev_window(payload, ctx, kind):
    return ctx.window_value(kind, payload)


def ev_aggregate(payload, ctx):
    return ctx.aggregate_value(payload)


def ev_odm(payload, ctx):
    """REQ-1265: one collected item read from the row's ODM scope."""
    return ctx.odm_value(payload)


def ev_function(payload, ctx):
    """Stage 2: call a project function (REQ-1085)."""
    from . import functions as _functions

    e = ctx.e
    if not getattr(e, "functions", None):
        _functions._fail(
            ctx.where, "validation", "project_environment_missing", "REQ-0694", {}
        )
    func = _functions.check_call(e.functions, payload, ctx.where)
    arg_values = {}
    for arg_name, arg_spec in (payload.get("args") or {}).items():
        arg_values[arg_name] = _resolve_function_arg(arg_spec, ctx)
    return _functions.call_function(func, arg_values, ctx.where)


def _resolve_function_arg(spec, ctx):
    """Resolve a function_arg: literal dict, variable name, or scalar."""
    if isinstance(spec, dict) and "literal" in spec:
        return spec["literal"]
    if isinstance(spec, str):
        # REQ-0679: a named variable, qualified or not, is read as a scalar
        # source; `compute`'s identifier restrictions do not apply here.
        return ctx.value(spec)
    return spec


_REGISTRY = {
    "source": ev_source,
    "literal": ev_literal,
    "first_available": ev_first_available,
    "greatest": lambda p, c: ev_greatest_least(p, c, "greatest"),
    "least": lambda p, c: ev_greatest_least(p, c, "least"),
    "case": ev_case,
    "flag": ev_flag,
    "str_pad": ev_str_pad,
    "mapping": ev_mapping,
    "cut": ev_cut,
    "str_extract": ev_str_extract,
    "str_concat": ev_str_concat,
    "str_template": ev_str_template,
    "str_case": ev_str_case,
    "str_contains": ev_str_contains,
    "compute": ev_compute,
    "round_half_away_from_zero": ev_round_half_away_from_zero,
    "date_diff": ev_date_diff,
    "date_impute": ev_date_impute,
    "date_precision": ev_date_precision,
    "datetime_impute": ev_datetime_impute,
    "datetime_precision": ev_datetime_precision,
    "to_epoch_day": ev_to_epoch_day,
    "to_date": ev_to_date,
    "study_day": ev_study_day,
    "row_number": lambda p, c: ev_window(p, c, "row_number"),
    "rank": lambda p, c: ev_window(p, c, "rank"),
    "row_value": lambda p, c: ev_window(p, c, "row_value"),
    "previous_non_missing": lambda p, c: ev_window(p, c, "previous_non_missing"),
    "baseline_flag": lambda p, c: ev_window(p, c, "baseline_flag"),
    "aggregate": ev_aggregate,
    "odm": ev_odm,
    "function": ev_function,
}
