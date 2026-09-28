"""R004 predicate language: tokenizer, recursive-descent parser, evaluator."""

import re

from .errors import YamaaError
from .values import (
    comparable,
    compare,
    is_missing,
    normalize_number,
    parse_date,
    parse_datetime,
)

KEYWORDS = {
    "AND",
    "OR",
    "NOT",
    "IN",
    "BETWEEN",
    "LIKE",
    "ESCAPE",
    "IS",
    "NULL",
    "TRUE",
    "FALSE",
    "DATE",
    "DATETIME",
}

# ------------------------------------------------- portable regex (R022)
# Shared with validate.py: the pure grammar test and the Python-`re`
# normalization live here so both the predicate grammar (REQ-1244) and
# the schema-level pattern checks use one definition.

_ESCAPE_OK = set("dDsSwWbBnrtfvxu0123456789") | set("^$\\.*+?()[]{}|/-'\" ")


def portable_pattern_error(pattern):
    """R022: return a detail string when the portable regex grammar rejects
    `pattern`, else None."""
    bad = None
    if "(?P<" in pattern:
        bad = "named group (?P<name> is outside the portable grammar"
    elif re.search(r"\(\?[^<:=!]", pattern):
        bad = "inline flag group is outside the portable grammar"
    elif "\\p{" in pattern or "\\P{" in pattern:
        bad = "property escape is outside the portable grammar"
    else:
        i = 0
        n = len(pattern)
        in_class = False
        while i < n:
            c = pattern[i]
            if c == "\\":
                i += 1
                if i >= n:
                    bad = "trailing escape"
                    break
                nc = pattern[i]
                if nc == "u" and i + 1 < n and pattern[i + 1] == "{":
                    j = pattern.find("}", i + 2)
                    if j < 0 or not re.fullmatch(r"[0-9a-fA-F]+", pattern[i + 2 : j]):
                        bad = "malformed \\u{...} escape"
                        break
                    i = j + 1
                    continue
                if nc not in _ESCAPE_OK and not in_class:
                    bad = f"malformed escape \\{nc}"
                    break
                i += 1
                continue
            if c == "[" and not in_class:
                in_class = True
            elif c == "]" and in_class:
                in_class = False
            elif (
                c == "{"
                and not in_class
                and not re.match(r"\{\d+(,\d*)?\}", pattern[i:])
            ):
                bad = "lone quantifier brace is outside the portable grammar"
                break
            i += 1
    if bad is None:
        try:
            normalize_pattern(pattern)
        except re.error as ex:
            bad = f"pattern does not compile: {ex}"
    return bad


# REQ-0823: \s is exactly the ECMA-262 WhiteSpace + LineTerminator set.
_ECMA_S = "\\t\\v\\f \\u00A0\\u1680\\u2000-\\u200A\\u202F\\u205F\\u3000\\uFEFF\\n\\r\\u2028\\u2029"


def normalize_pattern(pattern):
    """Apply R022's normalization for Python's re: (?<name>) groups, \\u{...},
    '.' excluding U+2028/29, '$' at end only. Returns the compiled pattern."""
    out = []
    i, n = 0, len(pattern)
    in_class = False
    while i < n:
        c = pattern[i]
        if c == "\\":
            nc = pattern[i + 1] if i + 1 < n else ""
            if nc == "u" and i + 2 < n and pattern[i + 2] == "{":
                j = pattern.find("}", i + 3)
                if j < 0:
                    raise re.error("unclosed \\u{...} escape")
                cp = int(pattern[i + 3 : j], 16)
                out.append(f"\\U{cp:08X}")
                i = j + 1
                continue
            if nc == "s" and not in_class:
                out.append("[" + _ECMA_S + "]")
                i += 2
                continue
            if nc == "S" and not in_class:
                out.append("[^" + _ECMA_S + "]")
                i += 2
                continue
            out.append(c + nc if nc else c)
            i += 2 if nc else 1
            continue
        if c == "[" and not in_class:
            in_class = True
            out.append(c)
            i += 1
            continue
        if c == "]" and in_class:
            in_class = False
            out.append(c)
            i += 1
            continue
        if not in_class:
            if c == ".":
                out.append("[^\\u000A\\u000D\\u2028\\u2029]")
                i += 1
                continue
            if c == "$":
                out.append("\\Z")
                i += 1
                continue
            if pattern.startswith("(?<", i) and (
                i + 3 >= n or pattern[i + 3] not in "=!"
            ):
                # (?<=...) and (?<!...) are lookbehind: pass through to re.
                j = pattern.find(">", i + 3)
                if j < 0:
                    raise re.error("unclosed (?<name> group")
                out.append("(?P<" + pattern[i + 3 : j] + ">")
                i = j + 1
                continue
        out.append(c)
        i += 1
    return re.compile("".join(out))


_TOKEN_RE = re.compile(
    r"""(?P<ws>\s+)
    |(?P<dotted>[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*)
    |(?P<word>[A-Za-z_][A-Za-z0-9_]*)
    |(?P<string>'(?:[^']|'')*')
    |(?P<number>[+-]?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)
    |(?P<op><=|>=|<>|=|<|>)
    |(?P<punct>[(),])
    """,
    re.VERBOSE,
)


def tokenize(text, where="<predicate>"):
    toks = []
    pos = 0
    while pos < len(text):
        m = _TOKEN_RE.match(text, pos)
        if not m:
            raise YamaaError(
                phase="validation",
                condition="invalid_predicate",
                requirement="R004-31",
                spec_paths=[where],
                context={"text": text, "at": text[pos : pos + 20]},
            )
        pos = m.end()
        kind = m.lastgroup
        val = m.group()
        if kind == "ws":
            continue
        if kind == "word" and val.upper() in KEYWORDS:
            toks.append(("kw", val.upper()))
        elif kind == "dotted":
            toks.append(("ident", val))
        else:
            toks.append((kind, val))
    return toks


class Parser:
    def __init__(self, text, where):
        self.toks = tokenize(text, where)
        self.pos = 0
        self.where = where
        self.text = text
        self.names = []  # identifiers bound, in order

    def peek(self):
        return self.toks[self.pos] if self.pos < len(self.toks) else (None, None)

    def next(self):
        t = self.peek()
        self.pos += 1
        return t

    def expect_kw(self, kw):
        kind, val = self.next()
        if kind != "kw" or val != kw:
            self.fail(f"expected {kw}")
        return val

    def fail(self, msg="parse error"):
        raise YamaaError(
            phase="validation",
            condition="invalid_predicate",
            requirement="R004-31",
            spec_paths=[self.where],
            context={"text": self.text, "detail": msg},
        )

    def parse(self):
        node = self.disjunction()
        if self.pos != len(self.toks):
            self.fail("trailing tokens")
        return node

    def disjunction(self):
        node = self.conjunction()
        while self.peek() == ("kw", "OR"):
            self.next()
            node = ("or", node, self.conjunction())
        return node

    def conjunction(self):
        node = self.negation()
        while self.peek() == ("kw", "AND"):
            self.next()
            node = ("and", node, self.negation())
        return node

    def negation(self):
        if self.peek() == ("kw", "NOT"):
            self.next()
            return ("not", self.negation())
        return self.boolean()

    def boolean(self):
        kind, val = self.peek()
        if kind == "kw" and val == "TRUE":
            self.next()
            return ("lit", True)
        if kind == "kw" and val == "FALSE":
            self.next()
            return ("lit", False)
        if kind == "punct" and val == "(":
            self.next()
            node = self.disjunction()
            k, v = self.next()
            if k != "punct" or v != ")":
                self.fail("expected )")
            return node
        if (
            kind in ("ident", "word")
            and self.pos + 1 < len(self.toks)
            and self.toks[self.pos + 1] == ("punct", "(")
        ):
            # REQ-1244: str_contains is the sole Boolean function the
            # grammar admits. Name matching is case-insensitive; a bare
            # str_contains without '(' falls through and stays an
            # identifier; any other name(...) is invalid_predicate.
            if val.upper() == "STR_CONTAINS":
                return self._str_contains_call()
            raise YamaaError(
                phase="validation",
                condition="invalid_predicate",
                requirement="REQ-1244",
                spec_paths=[self.where],
                context={"text": self.text, "name": val},
            )
        return self.comparison_or_null_test()

    def _str_contains_call(self):
        _, _name = self.next()  # the function word
        self.next()  # '('
        src = self._operand()  # REQ-1244: source is any operand
        k, v = self.next()
        if k != "punct" or v != ",":
            self.fail("expected , in str_contains call")

        def bad_pattern(detail):
            raise YamaaError(
                phase="validation",
                condition="invalid_predicate",
                requirement="REQ-1244",
                spec_paths=[self.where],
                context={"text": self.text, "detail": detail},
            )

        k, v = self.next()
        # REQ-1244: the pattern is a string literal holding a portable regex.
        if k != "string":
            bad_pattern("str_contains pattern must be a string literal")
        pattern = v[1:-1].replace("''", "'")
        bad = portable_pattern_error(pattern)
        if bad is not None:
            bad_pattern(f"pattern outside the portable grammar: {bad}")
        k, v = self.next()
        if k != "punct" or v != ")":
            self.fail("expected ) in str_contains call")
        return ("str_contains", src, pattern)

    def _operand(self):
        kind, val = self.next()
        if kind == "ident":
            self.names.append(val)
            return ("ident", val)
        if kind == "word":
            # bare name (non-keyword, since keywords tokenize as kw)
            self.names.append(val)
            return ("ident", val)
        if kind == "string":
            return ("lit", val[1:-1].replace("''", "'"), val)
        if kind == "number":
            if "." in val or "e" in val or "E" in val:
                return ("lit", normalize_number(float(val)), val)
            return ("lit", int(val), val)
        if kind == "kw" and val == "NULL":
            return ("lit", None, "NULL")
        if kind == "kw" and val in ("DATE", "DATETIME"):
            k2, v2 = self.next()
            if k2 != "string":
                self.fail("temporal literal needs a string")
            text = v2[1:-1].replace("''", "'")
            try:
                parsed = parse_date(text) if val == "DATE" else parse_datetime(text)
            except ValueError:
                raise YamaaError(
                    phase="derivation",
                    condition="invalid_date_text",
                    requirement="R004-35",
                    spec_paths=[self.where],
                    context={"text": self.text, "literal": text},
                )
            return ("lit", parsed, text)
        self.fail(f"bad operand {val!r}")

    def comparison_or_null_test(self):
        left = self._operand()
        kind, val = self.peek()
        if kind == "kw" and val == "IS":
            self.next()
            neg = False
            if self.peek() == ("kw", "NOT"):
                self.next()
                neg = True
            self.expect_kw("NULL")
            return ("isnotnull" if neg else "isnull", left)
        neg = False
        if kind == "kw" and val == "NOT":
            self.next()
            neg = True
            kind, val = self.peek()
        if kind == "kw" and val in ("IN", "BETWEEN", "LIKE"):
            self.next()
            if val == "IN":
                k, v = self.next()
                if k != "punct" or v != "(":
                    self.fail("expected ( after IN")
                items = [self._operand()]
                while self.peek() == ("punct", ","):
                    self.next()
                    items.append(self._operand())
                k, v = self.next()
                if k != "punct" or v != ")":
                    self.fail("expected ) after IN list")
                node = ("in", left, items)
            elif val == "BETWEEN":
                lo = self._operand()
                self.expect_kw("AND")
                hi = self._operand()
                node = ("between", left, lo, hi)
            else:  # LIKE
                pat = self._operand()
                esc = None
                if self.peek() == ("kw", "ESCAPE"):
                    self.next()
                    k, v = self.next()
                    if k != "string":
                        self.fail("ESCAPE needs a string literal")
                    esc = v[1:-1].replace("''", "'")
                    if len(esc) != 1:
                        raise YamaaError(
                            phase="validation",
                            condition="invalid_predicate",
                            requirement="R004-34",
                            spec_paths=[self.where],
                            context={"text": self.text},
                        )
                node = ("like", left, pat, esc)
            return ("not", node) if neg else node
        if kind == "op":
            self.next()
            right = self._operand()
            node = ("cmp", val, left, right)
            return ("not", node) if neg else node
        self.fail(f"expected comparison after operand, got {val!r}")


def parse(text, where="<predicate>"):
    p = Parser(text, where)
    return p.parse(), p.names


def _not(v):
    return None if v is None else (not v)


def _and(a, b):
    if a is False or b is False:
        return False
    if a is True and b is True:
        return True
    return None


def _or(a, b):
    if a is True or b is True:
        return True
    if a is False and b is False:
        return False
    return None


def _like_match(value, pattern, escape):
    # R004-19/20: % any sequence, _ one scalar, case-sensitive; ESCAPE one scalar.
    i, j = 0, 0
    star = -1
    match = 0
    n, m = len(value), len(pattern)
    while i < n:
        if j < m:
            pc = pattern[j]
            if escape is not None and pc == escape:
                j += 1
                if j >= m:
                    raise YamaaError(
                        phase="validation",
                        condition="invalid_predicate",
                        requirement="R004-34",
                        context={"pattern": pattern},
                    )
                if value[i] == pattern[j]:
                    i += 1
                    j += 1
                    continue
                return False
            if pc == "%":
                star = j
                match = i
                j += 1
                continue
            if pc == "_" or value[i] == pc:
                i += 1
                j += 1
                continue
        if star != -1:
            match += 1
            i = match
            j = star + 1
            continue
        return False
    while j < m and pattern[j] == "%":
        j += 1
    return j == m


def evaluate(node, resolve, where="<predicate>"):
    """resolve(name) -> value. Returns True/False/None."""
    kind = node[0]
    if kind == "lit":
        return node[1]
    if kind == "ident":
        return resolve(node[1])
    if kind == "not":
        return _not(evaluate(node[1], resolve, where))
    if kind == "and":
        return _and(
            evaluate(node[1], resolve, where), evaluate(node[2], resolve, where)
        )
    if kind == "or":
        return _or(evaluate(node[1], resolve, where), evaluate(node[2], resolve, where))
    if kind == "isnull":
        return is_missing(evaluate(node[1], resolve, where))
    if kind == "isnotnull":
        return not is_missing(evaluate(node[1], resolve, where))
    if kind == "cmp":
        _, op, l, r = node
        lv, rv = evaluate(l, resolve, where), evaluate(r, resolve, where)
        if is_missing(lv) or is_missing(rv):
            return None  # R004-13
        if not comparable(lv, rv):
            raise YamaaError(
                phase="derivation",
                condition="incompatible_input_type",
                requirement="R004-33",
                spec_paths=[where],
                context={"op": op},
            )
        c = compare(lv, rv)
        return {
            "=": c == 0,
            "<>": c != 0,
            "<": c < 0,
            "<=": c <= 0,
            ">": c > 0,
            ">=": c >= 0,
        }[op]
    if kind == "in":
        _, left, items = node
        result = False
        for item in items:
            r = evaluate(("cmp", "=", left, item), resolve, where)
            result = _or(result, r)
        return result
    if kind == "between":
        _, x, lo, hi = node
        return _and(
            evaluate(("cmp", ">=", x, lo), resolve, where),
            evaluate(("cmp", "<=", x, hi), resolve, where),
        )
    if kind == "like":
        _, v, pat, esc = node
        sv, pv = evaluate(v, resolve, where), evaluate(pat, resolve, where)
        if is_missing(sv) or is_missing(pv):
            return None
        if not isinstance(sv, str) or not isinstance(pv, str):
            raise YamaaError(
                phase="derivation",
                condition="incompatible_input_type",
                requirement="R004-33",
                spec_paths=[where],
            )
        return _like_match(sv, pv, esc)
    if kind == "str_contains":
        # REQ-1244: TRUE when the regex matches anywhere, FALSE when it
        # does not, UNKNOWN when the source is missing (including under
        # NOT); a non-str source is incompatible_input_type.
        _, src, pattern = node
        sv = evaluate(src, resolve, where)
        if is_missing(sv):
            return None
        if not isinstance(sv, str):
            raise YamaaError(
                phase="derivation",
                condition="incompatible_input_type",
                requirement="REQ-1244",
                spec_paths=[where],
            )
        return normalize_pattern(pattern).search(sv) is not None
    raise AssertionError(f"bad predicate node {kind}")
