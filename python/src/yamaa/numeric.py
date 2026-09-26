"""R010 numeric_expression: closed arithmetic grammar over named variables."""

import math
import re

from .errors import YamaaError
from .values import INT64_MAX, INT64_MIN, is_missing, normalize_number

_FUNCS = {
    "ABS": (1, 1),
    "CEIL": (1, 1),
    "FLOOR": (1, 1),
    "TRUNC": (1, 1),
    "SQRT": (1, 1),
    "POWER": (2, 2),
    "EXP": (1, 1),
    "LN": (1, 1),
    "MOD": (2, 2),
    "GREATEST": (2, None),
    "LEAST": (2, None),
    "NULLIF": (2, 2),
    "COALESCE": (1, None),
}

_TOKEN_RE = re.compile(
    r"""(?P<ws>\s+)
    |(?P<dotted>[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*)
    |(?P<word>[A-Za-z_][A-Za-z0-9_]*)
    |(?P<number>\d+(?:\.\d+)?)
    |(?P<cmpop><=|>=|<>|!=|=|<|>)
    |(?P<op>[+\-*/(),.])
    """,
    re.VERBOSE,
)


def tokenize(text, where):
    toks = []
    pos = 0
    while pos < len(text):
        m = _TOKEN_RE.match(text, pos)
        if not m:
            raise YamaaError(
                phase="validation",
                condition="invalid_predicate",
                requirement="R010-35",
                spec_paths=[where],
                context={"text": text},
            )
        pos = m.end()
        kind = m.lastgroup
        if kind == "ws":
            continue
        toks.append((kind, m.group()))
    return toks


class Parser:
    def __init__(self, text, where):
        self.toks = tokenize(text, where)
        self.pos = 0
        self.where = where
        self.text = text
        self.names = []

    def peek(self):
        return self.toks[self.pos] if self.pos < len(self.toks) else (None, None)

    def next(self):
        t = self.peek()
        self.pos += 1
        return t

    def fail(self, msg="parse error"):
        raise YamaaError(
            phase="validation",
            condition="invalid_predicate",
            requirement="R010-35",
            spec_paths=[self.where],
            context={"text": self.text, "detail": msg},
        )

    def parse(self):
        for kind, text in self.toks:
            if kind == "cmpop":
                # R010-37: comparison operators are not numeric syntax.
                raise YamaaError(
                    phase="validation",
                    condition="prohibited_construct",
                    requirement="R010-37",
                    spec_paths=[self.where],
                    context={"expr": self.text, "operator": text},
                )
        node = self.expr()
        if self.pos != len(self.toks):
            self.fail("trailing tokens")
        return node

    def expr(self):
        node = self.term()
        while self.peek()[1] in ("+", "-"):
            op = self.next()[1]
            node = ("binop", op, node, self.term())
        return node

    def term(self):
        node = self.factor()
        while self.peek()[1] in ("*", "/"):
            op = self.next()[1]
            node = ("binop", op, node, self.factor())
        return node

    def factor(self):
        if self.peek()[1] in ("+", "-"):
            op = self.next()[1]
            return ("unary", op, self.factor())
        return self.primary()

    def primary(self):
        kind, val = self.next()
        if kind == "number":
            if "." in val or "e" in val or "E" in val:
                return ("lit", normalize_number(float(val)))
            return ("lit", int(val))
        if kind == "word" and val.upper() == "NULL":
            return ("lit", None)
        if kind in ("word", "dotted"):
            name = val
            if self.peek() == ("op", "("):
                return self.call(name)
            self.names.append(name)
            return ("ident", name)
        if kind == "op" and val == "(":
            node = self.expr()
            k, v = self.next()
            if (k, v) != ("op", ")"):
                self.fail("expected )")
            return node
        self.fail(f"bad primary {val!r}")

    def call(self, name):
        fname = name.upper()
        if fname not in _FUNCS:
            raise YamaaError(
                phase="validation",
                condition="prohibited_function",
                requirement="R010-36",
                spec_paths=[self.where],
                context={"text": self.text, "function": name},
            )
        self.next()  # (
        args = []
        if self.peek() != ("op", ")"):
            args.append(self.expr())
            while self.peek() == ("op", ","):
                self.next()
                args.append(self.expr())
        k, v = self.next()
        if (k, v) != ("op", ")"):
            self.fail("expected ) after call")
        lo, hi = _FUNCS[fname]
        if len(args) < lo or (hi is not None and len(args) > hi):
            raise YamaaError(
                phase="validation",
                condition="prohibited_function",
                requirement="R010-36",
                spec_paths=[self.where],
                context={"text": self.text, "function": name, "argc": len(args)},
            )
        return ("call", fname, args)


def parse(text, where="<numeric>"):
    p = Parser(text, where)
    return p.parse(), p.names


def _check_int(v, where, expr_text):
    if not (INT64_MIN <= v <= INT64_MAX):
        raise YamaaError(
            phase="derivation",
            condition="integer_overflow",
            requirement="R010-30",
            spec_paths=[where],
            context={"expr": expr_text},
        )
    return v


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def _num(v, where, expr_text):
    if is_missing(v):
        return None
    if isinstance(v, bool) or not (_is_int(v) or isinstance(v, float)):
        raise YamaaError(
            phase="derivation",
            condition="incompatible_input_type",
            requirement="R010-40",
            spec_paths=[where],
            context={"expr": expr_text},
        )
    return v


def _binop(op, a, b, where, expr_text):
    a = _num(a, where, expr_text)
    b = _num(b, where, expr_text)
    if a is None or b is None:
        return None  # R010-22 NULL propagates
    if op == "/":
        if b == 0:
            raise YamaaError(
                phase="derivation",
                condition="division_by_zero",
                requirement="R010-26",
                spec_paths=[where],
                context={"expr": expr_text},
            )
        return normalize_number(float(a) / float(b))
    if op == "+":
        r = a + b
    elif op == "-":
        r = a - b
    elif op == "*":
        r = a * b
    if _is_int(a) and _is_int(b):
        return _check_int(r, where, expr_text)
    return normalize_number(float(r))


def _call(fname, args, where, expr_text):
    vals = [_num(a, where, expr_text) for a in args]
    if fname == "COALESCE":
        for v in vals:
            if v is not None:
                return v
        return None
    if fname == "NULLIF":
        x, y = vals
        if x is None or y is None:
            return x
        eq = (
            (float(x) == float(y))
            if isinstance(x, float) or isinstance(y, float)
            else (x == y)
        )
        return None if eq else x
    if fname in ("GREATEST", "LEAST"):
        present = [v for v in vals if v is not None]
        if not present:
            return None
        nums = [_num(v, where, expr_text) for v in present]
        best = max(nums) if fname == "GREATEST" else min(nums)
        # R010-19: promoted type - int iff every argument int
        if all(_is_int(v) for v in present):
            return best
        return normalize_number(float(best))
    x = vals[0]
    if x is None:
        return None
    if fname == "ABS":
        return abs(x)
    if fname == "CEIL":
        return normalize_number(float(math.ceil(x)))
    if fname == "FLOOR":
        return normalize_number(float(math.floor(x)))
    if fname == "TRUNC":
        return normalize_number(float(math.trunc(x)))
    if fname == "SQRT":
        if x < 0:
            raise YamaaError(
                phase="derivation",
                condition="sqrt_of_negative",
                requirement="R010-27",
                spec_paths=[where],
                context={"expr": expr_text},
            )
        return normalize_number(math.sqrt(x))
    if fname == "EXP":
        return normalize_number(math.exp(x))
    if fname == "LN":
        if x <= 0:
            raise YamaaError(
                phase="derivation",
                condition="ln_of_nonpositive",
                requirement="R010-28",
                spec_paths=[where],
                context={"expr": expr_text},
            )
        return normalize_number(math.log(x))
    if fname == "POWER":
        y = vals[1]
        if y is None:
            return None
        if x == 0 and y < 0:
            raise YamaaError(
                phase="derivation",
                condition="division_by_zero",
                requirement="R010-29",
                spec_paths=[where],
                context={"expr": expr_text},
            )
        if x < 0 and not (isinstance(y, float) and y.is_integer()) and not _is_int(y):
            raise YamaaError(
                phase="derivation",
                condition="invalid_power",
                requirement="R010-29",
                spec_paths=[where],
                context={"expr": expr_text},
            )
        return normalize_number(math.pow(x, y))
    if fname == "MOD":
        y = vals[1]
        if y is None:
            return None
        if y == 0:
            raise YamaaError(
                phase="derivation",
                condition="division_by_zero",
                requirement="R010-26",
                spec_paths=[where],
                context={"expr": expr_text},
            )
        # remainder taking the sign of x: x - y*trunc(x/y)
        q = math.trunc(x / y)
        r = x - q * y
        if _is_int(x) and _is_int(y):
            return _check_int(r, where, expr_text)
        return normalize_number(float(r))
    raise AssertionError(fname)


def evaluate(node, resolve, where="<numeric>", expr_text=""):
    kind = node[0]
    if kind == "lit":
        return node[1]
    if kind == "ident":
        return resolve(node[1])
    if kind == "unary":
        v = _num(evaluate(node[2], resolve, where, expr_text), where, expr_text)
        if v is None:
            return None
        r = -v if node[1] == "-" else v
        if _is_int(v):
            return _check_int(r, where, expr_text)
        return normalize_number(r)
    if kind == "binop":
        return _binop(
            node[1],
            evaluate(node[2], resolve, where, expr_text),
            evaluate(node[3], resolve, where, expr_text),
            where,
            expr_text,
        )
    if kind == "call":
        return _call(
            node[1],
            [evaluate(a, resolve, where, expr_text) for a in node[2]],
            where,
            expr_text,
        )
    raise AssertionError(f"bad numeric node {kind}")


def round_half_away_from_zero(x, digits):
    """REQ-0418: round to `digits` places, ties half away from zero."""
    factor = 10.0**digits
    scaled = float(x) * factor
    tol = math.sqrt(2.0**-52)
    ax = abs(scaled)
    fl = math.floor(ax)
    n = fl + 1 if (ax - fl) >= 0.5 - tol else fl
    r = math.copysign(n, scaled) / factor
    if r == 0:
        return 0.0
    return float(r)
