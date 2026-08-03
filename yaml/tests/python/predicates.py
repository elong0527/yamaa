"""R004 predicates: the documented core subset, and nothing beyond it.

Shared by validate.py, which checks a predicate's syntax and the columns it
names, and by engine.py, which evaluates it under SQL three-valued logic.
"""
from __future__ import annotations

import re


class DerivationError(Exception):
    """A specification is invalid, or asks for behavior no rule defines."""


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


def evaluate_predicate(text, lookup):
    """True only when the predicate is TRUE; FALSE and UNKNOWN both reject."""
    return _Pred(tokenize(text), lookup).parse() is True


def check_syntax(text):
    """Parse without needing data. Raises DerivationError on bad syntax."""
    _Pred(tokenize(text), lambda _n: None).parse()
