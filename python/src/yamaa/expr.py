"""Expression registry evaluation (R007). One function per expression family."""

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


def _fail(where, phase, condition, requirement, context=None):
    raise YamaaError(
        phase=phase,
        condition=condition,
        requirement=requirement,
        spec_paths=[where],
        context=context or {},
    )


def eval_expr(node, ctx):
    """node: a derivation mapping (one registry key) or {'value':...} handled form."""
    if (
        isinstance(node, dict)
        and "value" in node
        and set(node) <= {"value", "missing", "strict"}
    ):
        return eval_expr(node["value"], ctx)
    if not isinstance(node, dict) or len(node) != 1:
        _fail(
            ctx.where, "validation", "invalid_field_type", "R007", {"derivation": node}
        )
    key = next(iter(node))
    if key == "value":  # handled_expression_class wrapper (R008)
        return eval_expr(node["value"], ctx)
    fn = _REGISTRY.get(key)
    if fn is None:
        _fail(ctx.where, "validation", "unknown_field", "R007", {"expression": key})
    return fn(node[key], ctx)


def _source_var_payload(payload):
    """Normalize the `source` expression payload to (variable, filter, missing,
    multiple_matches)."""
    if isinstance(payload, str):
        return payload, None, _ABSENT, None
    if not isinstance(payload, dict):
        _fail("<source>", "validation", "invalid_field_type", "R007", {})
    return (
        payload.get("variable"),
        payload.get("filter"),
        payload.get("missing", _ABSENT),
        payload.get("multiple_matches"),
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


def _one_record(var, filt, missing, mult, ctx, stage):
    """Resolve a filtered source to one value (R002/R003/R008)."""
    recs = ctx.source_records(var)
    if filt is not None:
        recs = [
            r for r in recs if ctx.record_predicate(filt, r, var.split(".")[0]) is True
        ]
        if not recs:
            return None  # R008-14: empty filtered result is an absent match
    value_field = ctx.value_field(var)
    if not recs:
        if missing is not _ABSENT:
            return missing  # R003-19 / R008-4
        return None
    if len(recs) > 1:
        if mult is None:
            if getattr(ctx, "_col_phase", False):
                present = [
                    r.get(value_field)
                    for r in recs
                    if not is_missing(r.get(value_field))
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
                        ctx.where,
                        "derivation",
                        "multiple_values_per_key",
                        "R001-44",
                        {
                            "identifier": var,
                            "value_count": len(distinct),
                            "keys": [{k: e.rows[ctx.i].get(k) for k in e.keys}],
                        },
                    )
                return distinct[0] if distinct else None
            _fail(
                ctx.where,
                stage,
                "multiple_matches",
                "R003-17",
                {"variable": var, "records": len(recs)},
            )
        recs = _choose(recs, mult, ctx)
    rec = recs[0]
    if value_field not in rec:
        _fail(ctx.where, "validation", "unknown_field", "R002-27", {"variable": var})
    return rec[value_field]


def _choose(recs, mult, ctx):
    order_by = mult.get("order_by", [])
    keep = mult.get("keep")
    if not order_by or keep is None:
        _fail(ctx.where, "join", "unpaired_fields", "R003-9", {})
    ordered = ctx.order_records(recs, order_by)
    return [ordered[0] if keep == "first" else ordered[-1]]


def ev_source(payload, ctx):
    var, filt, missing, mult = _source_var_payload(payload)
    if "." in var:
        return _one_record(var, filt, missing, mult, ctx, "join")
    return ctx.value(var)


def ev_literal(payload, ctx):
    return normalize_number(payload) if isinstance(payload, float) else payload


def ev_first_available(payload, ctx):
    for s in payload["sources"]:
        var, filt = _filtered_source_payload(s)
        v = (
            _one_record(var, filt, _ABSENT, None, ctx, "mapping")
            if "." in var
            else ctx.value(var)
        )
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
                "derivation",
                "incompatible_input_type",
                "R007-38",
                {"sources": payload["sources"]},
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
        mv = payload.get("missing_value", _ABSENT)
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
                spec_paths=e.spec_paths,
                context={"identifier": e.context.get("name")},
            ) from e
        raise
    if holds is True:
        return tv
    if holds is False:
        return None if fv is _ABSENT else fv
    return None if mv is _ABSENT else mv


def _raw_operand(var, ctx, stage="extract"):
    """Resolve an operand without handler substitution or type check."""
    if "." not in var:
        return ctx._match_operand(var)
    return _one_record(var, None, _ABSENT, None, ctx, stage)


def _canonical_text(v, var, ctx):
    """REQ-0010 scalar -> canonical text. Booleans fail conversion."""
    if isinstance(v, bool):
        _fail(
            ctx.where,
            "extract",
            "incompatible_input_type",
            "REQ-0010",
            {"variable": var},
        )
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
    _fail(
        ctx.where, "extract", "incompatible_input_type", "REQ-0010", {"variable": var}
    )


def ev_str_pad(payload, ctx):
    if not isinstance(payload, dict):
        _fail(
            ctx.where,
            "validation",
            "invalid_field_type",
            "REQ-1261",
            {"expected": "mapping"},
        )
    width = payload.get("width")
    if isinstance(width, bool) or not isinstance(width, int) or width < 1:
        _fail(
            ctx.where + ".width",
            "validation",
            "invalid_field_type",
            "REQ-1261",
            {"width": width},
        )
    var = payload.get("source")
    v = _raw_operand(var, ctx)
    if is_missing(v):
        return payload.get("missing")
    t = _canonical_text(v, var, ctx)
    return t if len(t) >= width else " " * (width - len(t)) + t


def ev_case(payload, ctx):
    for n, item in enumerate(payload):
        if not isinstance(item, dict):
            _fail(ctx.where, "validation", "invalid_field_type", "R007", {})
        if "when" in item:
            site = f"{ctx.where}.case[{n}].when"
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
                        context={"identifier": e.context.get("name")},
                    ) from e
                raise
            if holds is True:
                then = item["then"]
                if isinstance(then, str):
                    then = {"source": then}
                return eval_expr(then, ctx)
        elif "otherwise" in item and len(item) == 1:
            other = item["otherwise"]
            if isinstance(other, str):
                other = {"source": other}
            return eval_expr(other, ctx)
        else:
            _fail(ctx.where, "validation", "invalid_field_type", "R007", {})
    return None


def ev_mapping(payload, ctx):
    var, filt = _filtered_source_payload(payload["source"])
    v = (
        _one_record(var, filt, _ABSENT, None, ctx, "mapping")
        if "." in var
        else ctx.value(var)
    )
    strict = payload.get("strict", False) is True
    if is_missing(v):
        if strict:
            e = ctx.e
            _fail(
                ctx.where,
                "mapping",
                "missing_input",
                "REQ-0334",
                {"variable": var, "keys": [{k: e.rows[ctx.i].get(k) for k in e.keys}]},
            )
        return payload.get("missing")
    if not isinstance(v, str):
        _fail(ctx.where, "mapping", "incompatible_input_type", "R007", {})
    if "dict" in payload:
        d = payload["dict"]
    else:
        d = ctx.e._load_dict_yaml(payload["dict_yaml"], ctx.where)
    case_sensitive = payload.get("case_sensitive", True)
    if case_sensitive:
        key = v
    else:
        folded = {ascii_fold(k): k for k in d}
        if len(folded) != len(d):
            _fail(ctx.where, "validation", "ambiguous_dictionary", "R019-22", {})
        key = ascii_fold(v)
        d = {ascii_fold(k): val for k, val in d.items()}
    if key in d:
        return d[key]
    if strict:
        e = ctx.e
        _fail(
            ctx.where,
            "mapping",
            "unmapped_value",
            "REQ-0334",
            {
                "source": var,
                "value": v,
                "keys": [{k: e.rows[ctx.i].get(k) for k in e.keys}],
            },
        )
    if "unmapped" in payload:
        return payload["unmapped"]
    return payload.get("missing")


def ev_lookup(payload, ctx):
    return ctx.inline_lookup(payload)


def ev_cut(payload, ctx):
    src = payload["source"]
    v = (
        ctx.value(src)
        if "." not in src
        else _one_record(src, None, _ABSENT, None, ctx, "mapping")
    )
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
        _fail(ctx.where, "mapping", "incompatible_input_type", "R007", {})
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
    v = (
        ctx.value(src)
        if "." not in src
        else _one_record(src, None, _ABSENT, None, ctx, "round")
    )
    if is_missing(v):
        return None
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        _fail(
            ctx.where,
            "derivation",
            "incompatible_input_type",
            "REQ-0418",
            {"source": src, "value": v},
        )
    return _numeric.round_half_away_from_zero(v, payload["digits"])


def _str_operand(var, ctx, stage="extract"):
    """Resolve a string operand WITHOUT handler substitution."""
    v = (
        ctx.value(var)
        if "." not in var
        else _one_record(var, None, _ABSENT, None, ctx, stage)
    )
    if not is_missing(v) and not isinstance(v, str):
        _fail(ctx.where, stage, "incompatible_input_type", "R007", {"variable": var})
    return v


def ev_str_extract(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")  # None when unhandled -> missing result
    try:
        rx = re.compile(payload["pattern"])
    except re.error:
        _fail(
            ctx.where,
            "validation",
            "invalid_regex",
            "R022",
            {"pattern": payload["pattern"]},
        )
    m = rx.search(v)
    if not m:
        return payload.get("no_match")  # fatal when omitted (R008-3)
    g = payload.get("group", 0)
    try:
        return m.group(g)
    except IndexError:
        _fail(
            ctx.where,
            "extract",
            "regex_group_out_of_range",
            "R022",
            {"pattern": payload["pattern"], "group": g},
        )


def ev_str_concat(payload, ctx):
    parts = []
    for s in payload["sources"]:
        v = eval_expr(s, ctx)
        if is_missing(v):
            return payload.get("missing")  # fatal when omitted
        if not isinstance(v, str):
            _fail(ctx.where, "extract", "incompatible_input_type", "R007", {})
        parts.append(v)
    return "".join(parts)


_TEMPLATE_RE = re.compile(r"{{|}}|{[A-Za-z_][A-Za-z0-9_.]*}")


def ev_str_template(payload, ctx):
    template = payload if isinstance(payload, str) else payload["template"]
    missing = None if isinstance(payload, str) else payload.get("missing")
    pos = 0
    out = []
    for m in _TEMPLATE_RE.finditer(template):
        literal = template[pos : m.start()]
        if "{" in literal or "}" in literal:
            _fail(
                ctx.where,
                "validation",
                "invalid_string_template",
                "R012-16",
                {"template": template},
            )
        out.append(literal)  # literal text between tokens
        tok = m.group()
        if tok == "{{":
            out.append("{")
        elif tok == "}}":
            out.append("}")
        else:
            name = tok[1:-1]
            v = (
                ctx.value(name)
                if "." not in name
                else _one_record(name, None, _ABSENT, None, ctx, "template")
            )
            if is_missing(v):
                return missing  # fatal when omitted (R008-3)
            if not isinstance(v, str):
                _fail(
                    ctx.where,
                    "template",
                    "incompatible_input_type",
                    "R012-13",
                    {"variable": name},
                )
            out.append(v)
        pos = m.end()
    tail = template[pos:]
    if "{" in tail or "}" in tail:
        _fail(
            ctx.where,
            "validation",
            "invalid_string_template",
            "R012-16",
            {"template": template},
        )
    out.append(tail)
    return "".join(out)


def ev_str_upper(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")
    return ascii_upper(v)


def ev_str_contains(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")
    try:
        rx = _pred.normalize_pattern(payload["pattern"])
    except re.error:
        _fail(
            ctx.where,
            "validation",
            "invalid_regex",
            "R022",
            {"pattern": payload["pattern"]},
        )
    return rx.search(v) is not None


def ev_str_lower(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")
    return ascii_lower(v)


def ev_str_sentence(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")
    if not v:
        return v
    return ascii_upper(v[0]) + ascii_lower(v[1:])


def ev_str_title(payload, ctx):
    v = _str_operand(payload["source"], ctx)
    if is_missing(v):
        return payload.get("missing")
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


def ev_compute(payload, ctx):
    node, _ = _numeric.parse(payload["expr"], ctx.where)
    return _numeric.evaluate(node, ctx.compute_ident, ctx.where, payload["expr"])


def _date_operand(var, ctx, stage="impute"):
    """Resolve a date operand WITHOUT handler substitution (cf. _str_operand)."""
    return (
        ctx.value(var)
        if "." not in var
        else _one_record(var, None, _ABSENT, None, ctx, stage)
    )


def ev_date_diff(payload, ctx):
    s = (
        ctx.value(payload["start"])
        if "." not in payload["start"]
        else _one_record(payload["start"], None, _ABSENT, None, ctx, "derivation")
    )
    e = (
        ctx.value(payload["end"])
        if "." not in payload["end"]
        else _one_record(payload["end"], None, _ABSENT, None, ctx, "derivation")
    )
    if is_missing(s) or is_missing(e):
        return None
    if type(s) is not type(e) or not isinstance(s, (YDate, YDateTime)):
        _fail(ctx.where, "derivation", "incompatible_input_type", "R016-38", {})
    unit = payload["unit"]
    bounds = payload.get("bounds", "exclusive")
    if unit != "day" and bounds != "exclusive":
        _fail(
            ctx.where,
            "validation",
            "value_not_permitted",
            "R016",
            {"unit": unit, "bounds": bounds},
        )
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
    _fail(ctx.where, "validation", "value_not_permitted", "R016", {"unit": unit})


def _whole_months(s, e):
    """R016-73: count monthly anniversaries of s on or before e, with the
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


def ev_date_impute(payload, ctx):
    src = _date_operand(payload["source"], ctx)
    if is_missing(src):
        return payload.get("missing")
    if isinstance(src, YDate):
        completed, precision = src, src.precision
    elif isinstance(src, str):
        parts = date_prefix_parts(src)
        if parts is None:
            if "invalid" not in payload:
                _fail(ctx.where, "impute", "invalid_date_text", "R016", {"source": src})
            return payload["invalid"]
        year, month, day = parts
        if day is not None:
            try:
                completed = YDate(year, month, day)
            except ValueError:
                if "invalid" not in payload:
                    _fail(
                        ctx.where,
                        "impute",
                        "invalid_calendar_date",
                        "R016",
                        {"source": src},
                    )
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
            try:
                completed = YDate(year, month, d)
            except ValueError:
                if "invalid" not in payload:
                    _fail(
                        ctx.where,
                        "impute",
                        "invalid_calendar_date",
                        "R016",
                        {"source": src},
                    )
                return payload["invalid"]
            precision = "M" if parts[1] is not None else "Y"
    else:
        _fail(
            ctx.where,
            "impute",
            "incompatible_input_type",
            "R016",
            {"source": type(src).__name__},
        )
    nb = payload.get("not_before")
    if nb is not None:
        bound = (
            ctx.value(nb)
            if "." not in nb
            else _one_record(nb, None, _ABSENT, None, ctx, "impute")
        )
        if not is_missing(bound):
            if type(bound) is not YDate:
                _fail(ctx.where, "impute", "incompatible_input_type", "R016", {})
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
            if "invalid" not in payload:
                _fail(ctx.where, "impute", "invalid_date_text", "R016", {"source": src})
            return payload["invalid"]
        _, month, day = parts
        return "D" if day is not None else ("M" if month is not None else "Y")
    _fail(ctx.where, "impute", "incompatible_input_type", "R016", {})


def ev_to_date(payload, ctx):
    v = (
        ctx.value(payload["source"])
        if "." not in payload["source"]
        else _one_record(payload["source"], None, _ABSENT, None, ctx, "derivation")
    )
    if is_missing(v):
        return None
    if isinstance(v, YDateTime):
        return YDate(v.year, v.month, v.day)
    if isinstance(v, str):
        try:
            return parse_date(v)
        except ValueError:
            _fail(
                ctx.where,
                "derivation",
                "invalid_date_text",
                "REQ-1107",
                {"source": payload["source"], "text": v},
            )
    _fail(ctx.where, "derivation", "incompatible_input_type", "R016", {})


def ev_study_day(payload, ctx):
    d = (
        ctx.value(payload["date"])
        if "." not in payload["date"]
        else _one_record(payload["date"], None, _ABSENT, None, ctx, "derivation")
    )
    r = (
        ctx.value(payload["reference"])
        if "." not in payload["reference"]
        else _one_record(payload["reference"], None, _ABSENT, None, ctx, "derivation")
    )
    if is_missing(d) or is_missing(r):
        return None
    if type(d) is not type(r) or not isinstance(d, (YDate, YDateTime)):
        _fail(ctx.where, "derivation", "incompatible_input_type", "R016-38", {})
    delta = (d - r).days
    return delta + 1 if delta >= 0 else delta


_EPOCH_ORDINAL = _date(1970, 1, 1).toordinal()


def _runtime_type_name(v):
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


def _keys_ctx(ctx):
    e = ctx.e
    try:
        return [{k: e.rows[ctx.i].get(k) for k in e.keys}]
    except (AttributeError, IndexError, TypeError):
        return []


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
    v = (
        ctx.value(var)
        if "." not in var
        else _one_record(var, None, _ABSENT, None, ctx, "impute")
    )
    if is_missing(v):
        if "missing" in payload:
            return payload["missing"]
        _fail(
            ctx.where,
            "impute",
            "missing_input",
            "REQ-1182",
            {"operation": "datetime_impute", "source": var, "keys": _keys_ctx(ctx)},
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
                "keys": _keys_ctx(ctx),
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
    v = (
        ctx.value(var)
        if "." not in var
        else _one_record(var, None, _ABSENT, None, ctx, "impute")
    )
    if is_missing(v):
        if "missing" in payload:
            return payload["missing"]
        _fail(
            ctx.where,
            "impute",
            "missing_input",
            "REQ-1183",
            {"operation": "datetime_precision", "source": var, "keys": _keys_ctx(ctx)},
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
                "keys": _keys_ctx(ctx),
            },
        )
    return precision


def ev_to_epoch_day(payload, ctx):
    var = payload["source"]
    v = (
        ctx.value(var)
        if "." not in var
        else _one_record(var, None, _ABSENT, None, ctx, "derivation")
    )
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
        if "." in spec:
            return ctx.compute_ident(spec)
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
    "lookup": ev_lookup,
    "cut": ev_cut,
    "str_extract": ev_str_extract,
    "str_concat": ev_str_concat,
    "str_template": ev_str_template,
    "str_upper": ev_str_upper,
    "str_lower": ev_str_lower,
    "str_contains": ev_str_contains,
    "str_sentence": ev_str_sentence,
    "str_title": ev_str_title,
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
    "function": ev_function,
}
