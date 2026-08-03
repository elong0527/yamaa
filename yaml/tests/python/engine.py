"""A reference derivation engine.

Executes a specification against its declared inputs and returns output rows.
It exists to prove the YAML design is executable and that R and Python agree,
not to be fast or complete. It implements exactly what the rules define, and
raises on anything a rule leaves unresolved rather than guessing.
"""
from __future__ import annotations

import csv
import re
from pathlib import Path

import yaml

from validate import EXC, OPS, fold_ascii

ODM_CONTEXT = [
    "StudyOID",
    "SubjectKey",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
]


class DerivationError(Exception):
    pass


class Unmapped(Exception):
    """Raised by an operation that cannot produce a result (R008 `unmapped`)."""


# --------------------------------------------------------------------------
# R004 predicates: the documented core subset, and nothing beyond it.
# --------------------------------------------------------------------------

_TOKEN = re.compile(
    r"""\s*(?:
        (?P<str>'(?:[^']|'')*')
      | (?P<num>-?\d+(?:\.\d+)?)
      | (?P<op><>|<=|>=|=|<|>)
      | (?P<lparen>\()
      | (?P<rparen>\))
      | (?P<comma>,)
      | (?P<ident>[A-Za-z_][A-Za-z0-9_.]*)
    )""",
    re.VERBOSE,
)

_KEYWORDS = {"AND", "OR", "NOT", "IS", "NULL", "IN", "TRUE", "FALSE"}


def tokenize(text):
    out, pos = [], 0
    while pos < len(text):
        m = _TOKEN.match(text, pos)
        if not m:
            if text[pos:].strip() == "":
                break
            raise DerivationError(f"cannot tokenize predicate at {text[pos:]!r}")
        pos = m.end()
        kind = m.lastgroup
        val = m.group()
        val = val.strip()
        if kind == "ident" and val.upper() in _KEYWORDS:
            out.append(("kw", val.upper()))
        elif kind == "str":
            out.append(("lit", val[1:-1].replace("''", "'")))
        elif kind == "num":
            out.append(("lit", float(val) if "." in val else int(val)))
        else:
            out.append((kind, val))
    return out


def predicate_vars(text):
    """Identifiers referenced by a predicate. R001 dependency inference."""
    return [v for k, v in tokenize(text) if k == "ident"]


class _Pred:
    """Recursive-descent evaluator over SQL three-valued logic."""

    def __init__(self, tokens, lookup):
        self.t, self.i, self.lookup = tokens, 0, lookup

    def peek(self):
        return self.t[self.i] if self.i < len(self.t) else (None, None)

    def eat(self, kind=None, val=None):
        k, v = self.peek()
        if kind and k != kind or val and v != val:
            raise DerivationError(f"expected {val or kind}, got {v!r}")
        self.i += 1
        return v

    def parse(self):
        v = self.disjunction()
        if self.i != len(self.t):
            raise DerivationError(f"trailing tokens in predicate: {self.t[self.i:]!r}")
        return v

    def disjunction(self):
        v = self.conjunction()
        while self.peek() == ("kw", "OR"):
            self.eat()
            r = self.conjunction()
            v = True if True in (v, r) else (None if None in (v, r) else False)
        return v

    def conjunction(self):
        v = self.negation()
        while self.peek() == ("kw", "AND"):
            self.eat()
            r = self.negation()
            v = False if False in (v, r) else (None if None in (v, r) else True)
        return v

    def negation(self):
        if self.peek() == ("kw", "NOT"):
            self.eat()
            v = self.negation()
            return None if v is None else not v
        return self.comparison()

    def comparison(self):
        if self.peek() == ("lparen", "("):
            self.eat()
            v = self.disjunction()
            self.eat("rparen")
            return v
        left = self.operand()
        k, v = self.peek()
        if (k, v) == ("kw", "IS"):
            self.eat()
            negate = False
            if self.peek() == ("kw", "NOT"):
                self.eat()
                negate = True
            self.eat("kw", "NULL")
            missing = left is None or left == ""
            return (not missing) if negate else missing
        if (k, v) == ("kw", "IN"):
            self.eat()
            self.eat("lparen")
            members = [self.operand()]
            while self.peek() == ("comma", ","):
                self.eat()
                members.append(self.operand())
            self.eat("rparen")
            if left is None:
                return None
            return any(_eq(left, m) for m in members)
        if k == "op":
            self.eat()
            right = self.operand()
            if left is None or right is None:
                return None
            return _compare(v, left, right)
        raise DerivationError(f"expected a comparison operator, got {v!r}")

    def operand(self):
        k, v = self.peek()
        if k == "lit":
            self.eat()
            return v
        if k == "ident":
            self.eat()
            return self.lookup(v)
        if (k, v) in (("kw", "TRUE"), ("kw", "FALSE")):
            self.eat()
            return v == "TRUE"
        raise DerivationError(f"expected an operand, got {v!r}")


def _coerce_pair(a, b):
    if isinstance(a, str) and isinstance(b, (int, float)):
        try:
            return float(a), float(b)
        except ValueError:
            return a, str(b)
    if isinstance(b, str) and isinstance(a, (int, float)):
        try:
            return float(a), float(b)
        except ValueError:
            return str(a), b
    return a, b


def _eq(a, b):
    a, b = _coerce_pair(a, b)
    return a == b


def _compare(op, a, b):
    a, b = _coerce_pair(a, b)
    if op == "=":
        return a == b
    if op == "<>":
        return a != b
    if op == "<":
        return a < b
    if op == "<=":
        return a <= b
    if op == ">":
        return a > b
    return a >= b if op == ">=" else False


def evaluate_predicate(text, lookup):
    """True only when the predicate is TRUE; FALSE and UNKNOWN both reject."""
    return _Pred(tokenize(text), lookup).parse() is True


# --------------------------------------------------------------------------
# Type conversion (R005) and serialization
# --------------------------------------------------------------------------

_ISO_DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def convert(value, declared):
    if value is None or value == "":
        return None
    if declared == "str":
        return str(value)
    if declared == "int":
        if isinstance(value, bool):
            raise DerivationError("bool is not an int (R006)")
        try:
            f = float(value)
        except (TypeError, ValueError):
            raise DerivationError(f"cannot convert {value!r} to int")
        if f != int(f):
            raise DerivationError(f"cannot convert {value!r} to int without loss")
        return int(f)
    if declared == "float":
        try:
            return float(value)
        except (TypeError, ValueError):
            raise DerivationError(f"cannot convert {value!r} to float")
    if declared == "date":
        s = str(value)
        if not _ISO_DATE.match(s):
            raise DerivationError(f"{value!r} is not an ISO 8601 complete date")
        return s
    raise DerivationError(
        f"unknown column type {declared!r}; R005 leaves the type vocabulary open"
    )


def serialize(value):
    """Render for CSV comparison. R005 defines no serialization, so this is the
    engine's convention and both implementations must share it."""
    if value is None:
        return ""
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    if isinstance(value, float):
        if value == int(value):
            return str(int(value))
        return repr(round(value, 10)).rstrip("0").rstrip(".")
    return str(value)


# --------------------------------------------------------------------------
# Operations
# --------------------------------------------------------------------------


def op_mapping(value, args, ctx):
    table = args["dict"]
    if args.get("case_sensitive", _default("mapping", "case_sensitive")):
        if value in table:
            return table[value]
    else:
        folded = {fold_ascii(k): v for k, v in table.items()}
        if fold_ascii(value) in folded:
            return folded[fold_ascii(value)]
    raise Unmapped(value)


def op_mapping_from(value, args, ctx):
    rows = ctx.data[args["dataset"]]
    hits = [r for r in rows if r.get(args["key"]) == value]
    if len(hits) > 1:
        raise DerivationError(
            f"mapping_from: {args['dataset']} is not unique on {args['key']} for {value!r}"
        )
    if not hits:
        raise Unmapped(value)
    return hits[0][args["value"]]


def op_multiply(value, args, ctx):
    return float(value) * float(args["factor"])


def op_add(value, args, ctx):
    return float(value) + float(args["addend"])


def op_subtract(value, args, ctx):
    a, b = args["minuend"], args["subtrahend"]
    return None if a is None or b is None else float(a) - float(b)


def op_percent_change(value, args, ctx):
    base = args["base"]
    if base in (None, "") or float(base) == 0 or args["value"] is None:
        return None
    return 100.0 * (float(args["value"]) - float(base)) / float(base)


def op_coalesce(value, args, ctx):
    for v in args["values"]:
        if v not in (None, ""):
            return v
    return None


def op_cut(value, args, ctx):
    if value in (None, ""):
        raise Unmapped(value)
    breaks, labels = args["breaks"], args["labels"]
    if len(labels) != len(breaks) + 1:
        raise DerivationError("cut: labels must have len(breaks) + 1 entries")
    right = args.get("right", _default("cut", "right"))
    x = float(value)
    for i, b in enumerate(breaks):
        if (x <= b) if right else (x < b):
            return labels[i]
    return labels[-1]


def op_str_extract(value, args, ctx):
    m = re.search(args["pattern"], str(value))
    if not m:
        raise Unmapped(value)
    return m.group(args.get("group", _default("str_extract", "group")))


def op_date_diff(value, args, ctx):
    from datetime import date

    if args["start"] in (None, "") or args["end"] in (None, ""):
        return None
    a = date.fromisoformat(str(args["start"]))
    b = date.fromisoformat(str(args["end"]))
    unit = args["unit"]
    if unit == "day":
        return (b - a).days
    if unit == "week":
        return (b - a).days // 7
    months = (b.year - a.year) * 12 + (b.month - a.month) - (1 if b.day < a.day else 0)
    return months if unit == "month" else months // 12


def op_case(value, args, ctx):
    for branch in args["branches"]:
        if evaluate_predicate(branch["when"], ctx.row_lookup):
            return branch["then"]
    return args.get("else")


SCALAR_OPS = {
    "mapping": op_mapping,
    "mapping_from": op_mapping_from,
    "multiply": op_multiply,
    "add": op_add,
    "subtract": op_subtract,
    "percent_change": op_percent_change,
    "coalesce": op_coalesce,
    "cut": op_cut,
    "str_extract": op_str_extract,
    "date_diff": op_date_diff,
    "case": op_case,
}


def _default(op, arg):
    for a in OPS[op]["arguments"]:
        name, desc = next(iter(a.items()))
        if name == arg:
            return desc.get("default")
    return None


def _sort_key(v):
    if v is None or v == "":
        return (1, "")
    try:
        return (0, float(v))
    except (TypeError, ValueError):
        return (0, str(v))


def window_row_number(rows, args, ctx):
    order = [
        tuple(_sort_key(a) for a in argvals) for argvals in args["order_by_per_row"]
    ]
    ranked = sorted(range(len(rows)), key=lambda i: (order[i], i))
    out = [None] * len(rows)
    for rank, idx in enumerate(ranked, 1):
        out[idx] = rank
    return out


def window_baseline_flag(rows, args, ctx):
    dates = args["date_per_row"]
    refs = args["reference_date_per_row"]
    best, best_i = None, None
    for i, d in enumerate(dates):
        if d in (None, "") or refs[i] in (None, ""):
            continue
        if str(d) <= str(refs[i]) and (best is None or str(d) > str(best)):
            best, best_i = d, i
        elif best is not None and str(d) == str(best) and i != best_i:
            raise DerivationError("baseline_flag: tie for the baseline record")
    out = [None] * len(rows)
    if best_i is not None:
        out[best_i] = "Y"
    return out


def window_baseline_value(rows, args, ctx):
    vals = args["value_per_row"]
    flags = args["flag_per_row"]
    picked = next((vals[i] for i, f in enumerate(flags) if f == "Y"), None)
    return [picked] * len(rows)


WINDOW_OPS = {
    "row_number": window_row_number,
    "baseline_flag": window_baseline_flag,
    "baseline_value": window_baseline_value,
}

AGGREGATE_OPS = {
    "min": lambda vals: min((v for v in vals if v not in (None, "")), key=_sort_key, default=None),
    "max": lambda vals: max((v for v in vals if v not in (None, "")), key=_sort_key, default=None),
}
