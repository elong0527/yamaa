"""R013 aggregate_expression: R010's grammar plus reduction nodes."""

from . import numeric as _num
from .errors import YamaaError
from .values import comparable, compare, is_missing, normalize_number

_REDUCERS = {"SUM", "COUNT", "MIN", "MAX", "MEAN", "ONLY"}


class Parser(_num.Parser):
    def primary(self):
        kind, val = self.peek()
        if kind == "word" and val.upper() in _REDUCERS:
            return self.reduction()
        return super().primary()

    def reduction(self):
        _kind, val = self.next()
        rname = val.upper()
        k, v = self.next()
        if (k, v) != ("op", "("):
            self.fail("expected ( after reducer")
        # COUNT(D.*) form
        if self.peek()[0] in ("word", "dotted"):
            _k2, v2 = self.next()
            if self.peek() == ("op", "."):
                self.next()
                k3, v3 = self.next()
                if (k3, v3) == ("op", "*"):
                    k4, v4 = self.next()
                    if (k4, v4) != ("op", ")"):
                        self.fail("expected ) after COUNT(D.*)")
                    if rname != "COUNT":
                        self.fail("only COUNT takes D.*")
                    self.names.append(v2)  # the relation name
                    return ("starcount", v2)
                self.fail("bad star form")
            else:
                # single identifier argument, already consumed
                self.pos -= 1
        arg = self.expr()
        k, v = self.next()
        if (k, v) != ("op", ")"):
            self.fail("expected ) after reduction")
        _reject_nested(arg, self)
        return ("reduce", rname, arg)


def _reject_nested(node, parser):
    if node[0] == "reduce" or node[0] == "starcount":
        raise YamaaError(
            phase="validation",
            condition="prohibited_construct",
            requirement="R013-37",
            spec_paths=[parser.where],
            context={"text": parser.text},
        )
    for child in node[1:]:
        if isinstance(child, tuple):
            _reject_nested(child, parser)
        elif isinstance(child, list):
            for c in child:
                if isinstance(c, tuple):
                    _reject_nested(c, parser)


def parse(text, where="<aggregate>"):
    p = Parser(text, where)
    node = p.parse()
    return node, p.names


def _reduction_value(rname, argnode, records, field_of, where, expr_text):
    """Evaluate one reduction over a record list. field_of(record, name)->value."""
    if rname == "__STAR__":
        # R013-27: no record in the group -> missing (not zero)
        return len(records) if records else None
    vals = []
    for rec in records:
        v = _num.evaluate(argnode, lambda n, r=rec: field_of(r, n), where, expr_text)
        vals.append(v)
    present = [v for v in vals if not is_missing(v)]
    if rname == "COUNT" and argnode == ("star",):
        return len(records) if records else None  # R013-27
    if rname == "ONLY":
        if len(records) > 1:
            raise YamaaError(
                phase="row_construction",
                condition="multiple_values_per_key",
                requirement="REQ-0501",
                spec_paths=[where],
                context={"expr": expr_text, "record_count": len(records)},
            )
        if not records:
            return None
        return vals[0]
    if not records:
        return None  # R013-27: no record in the group -> missing
    if rname == "COUNT":
        return len(present)  # 0 when records exist but all missing
    if not present:
        return None  # SUM/MIN/MAX/MEAN over all-missing -> missing
    if rname == "SUM":
        acc = present[0]
        for v in present[1:]:
            acc = _num._binop("+", acc, v, where, expr_text)
        return acc
    if rname == "MIN" or rname == "MAX":
        best = present[0]
        for v in present[1:]:
            if not comparable(best, v):
                raise YamaaError(
                    phase="derivation",
                    condition="incompatible_input_type",
                    requirement="R013-46",
                    spec_paths=[where],
                    context={"expr": expr_text},
                )
            c = compare(v, best)
            if (rname == "MIN" and c < 0) or (rname == "MAX" and c > 0):
                best = v
        return best
    if rname == "MEAN":
        total = present[0]
        for v in present[1:]:
            total = _num._binop("+", total, v, where, expr_text)
        n = len(present)
        return _num._binop("/", total, n, where, expr_text)
    raise AssertionError(rname)


def collect_reductions(node, out):
    """Find reduction nodes; returns list of (node)."""
    if node[0] == "reduce" or node[0] == "starcount":
        out.append(node)
    else:
        for child in node[1:]:
            if isinstance(child, tuple):
                collect_reductions(child, out)
            elif isinstance(child, list):
                for c in child:
                    if isinstance(c, tuple):
                        collect_reductions(c, out)
    return out


def evaluate(node, resolve_grouped, reductions, where="<aggregate>", expr_text=""):
    """resolve_grouped(name) -> constant for group_by identifiers.
    reductions: id(node) -> value for each reduction node."""
    kind = node[0]
    if kind == "reduce" or kind == "starcount":
        return reductions[id(node)]
    if kind == "ident":
        return resolve_grouped(node[1])
    if kind == "lit":
        return node[1]
    if kind == "unary":
        v = _num._num(
            evaluate(node[2], resolve_grouped, reductions, where, expr_text),
            where,
            expr_text,
        )
        if v is None:
            return None
        r = -v if node[1] == "-" else v
        if isinstance(v, int) and not isinstance(v, bool):
            return _num._check_int(r, where, expr_text)
        return normalize_number(r)
    if kind == "binop":
        return _num._binop(
            node[1],
            evaluate(node[2], resolve_grouped, reductions, where, expr_text),
            evaluate(node[3], resolve_grouped, reductions, where, expr_text),
            where,
            expr_text,
        )
    if kind == "call":
        return _num._call(
            node[1],
            [
                evaluate(a, resolve_grouped, reductions, where, expr_text)
                for a in node[2]
            ],
            where,
            expr_text,
        )
    raise AssertionError(f"bad aggregate node {kind}")


def eval_over_records(
    node, records, field_of, grouped_consts, where="<aggregate>", expr_text=""
):
    """Full evaluation of an aggregate expression over one record group."""
    reds = collect_reductions(node, [])
    red_vals = {}
    for r in reds:
        if r[0] == "starcount":
            red_vals[id(r)] = _reduction_value(
                "__STAR__", None, records, field_of, where, expr_text
            )
        else:
            _, rname, arg = r
            red_vals[id(r)] = _reduction_value(
                rname, arg, records, field_of, where, expr_text
            )

    def resolve_grouped(n):
        try:
            return grouped_consts[n]
        except KeyError:
            raise YamaaError(
                phase="validation",
                condition="aggregate_identifier_not_grouped",
                requirement="R013-38",
                spec_paths=[where],
                context={"identifier": n},
            )

    return evaluate(node, resolve_grouped, red_vals, where, expr_text)
