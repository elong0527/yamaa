"""
SQL/Predicate parser for translating SQL WHERE clauses into Polars expressions.
Supports compound boolean logic (AND, OR, NOT), comparison operators (=, !=, <, <=, >, >=),
IS NULL, IS NOT NULL, IN (...), and type-safe casting against Polars schemas.
"""

import re
import polars as pl
from typing import Any, List, Optional, Tuple


class SQLParser:
    """
    Parses SQL-like WHERE conditions into Polars pl.Expr.
    Handles tokenization, precedence, nested parentheses, and type inference.
    """

    @staticmethod
    def parse_to_expr(sql_str: str, schema: Optional[dict] = None) -> pl.Expr:
        if not sql_str or not sql_str.strip():
            return pl.lit(True)

        sql_str = sql_str.strip()
        tokens = SQLParser._tokenize(sql_str)
        if not tokens:
            return pl.lit(True)

        expr, remaining = SQLParser._parse_or(tokens, schema)
        if remaining:
            raise ValueError(f"Syntax error in SQL expression near: {' '.join(remaining)}")
        return expr

    @staticmethod
    def _tokenize(sql_str: str) -> List[str]:
        pattern = r"""
            (?:\s+) |                               # Whitespace
            ('(?:''|[^'])*') |                      # Single-quoted string
            ("(?:""|[^"])*") |                      # Double-quoted string
            (\bAND\b|\bOR\b|\bNOT\b|\bIS\s+NOT\b|\bIS\b|\bNULL\b|\bIN\b|\bLIKE\b) |  # Keywords
            (<=|>=|!=|<>|==|=|<|>) |                # Comparison operators
            (\(|\)|,) |                             # Parentheses and commas
            ([^\s(),=<>!]+)                         # Identifiers, numbers, literals
        """
        raw_tokens = [
            m.group(0)
            for m in re.finditer(pattern, sql_str, re.IGNORECASE | re.VERBOSE)
            if m.group(0).strip()
        ]
        
        tokens: List[str] = []
        i = 0
        while i < len(raw_tokens):
            t = raw_tokens[i]
            if t.upper() == "IS" and i + 1 < len(raw_tokens) and raw_tokens[i + 1].upper() == "NOT":
                tokens.append("IS NOT")
                i += 2
            else:
                tokens.append(t)
                i += 1
        return tokens

    @staticmethod
    def _parse_or(tokens: List[str], schema: Optional[dict]) -> Tuple[pl.Expr, List[str]]:
        left, tokens = SQLParser._parse_and(tokens, schema)
        while tokens and tokens[0].upper() == "OR":
            tokens = tokens[1:]
            right, tokens = SQLParser._parse_and(tokens, schema)
            left = left | right
        return left, tokens

    @staticmethod
    def _parse_and(tokens: List[str], schema: Optional[dict]) -> Tuple[pl.Expr, List[str]]:
        left, tokens = SQLParser._parse_not(tokens, schema)
        while tokens and tokens[0].upper() == "AND":
            tokens = tokens[1:]
            right, tokens = SQLParser._parse_not(tokens, schema)
            left = left & right
        return left, tokens

    @staticmethod
    def _parse_not(tokens: List[str], schema: Optional[dict]) -> Tuple[pl.Expr, List[str]]:
        if tokens and tokens[0].upper() == "NOT":
            tokens = tokens[1:]
            expr, tokens = SQLParser._parse_not(tokens, schema)
            return ~expr, tokens
        return SQLParser._parse_primary(tokens, schema)

    @staticmethod
    def _parse_primary(tokens: List[str], schema: Optional[dict]) -> Tuple[pl.Expr, List[str]]:
        if not tokens:
            return pl.lit(True), []

        token = tokens[0]

        if token == "(":
            expr, tokens = SQLParser._parse_or(tokens[1:], schema)
            if tokens and tokens[0] == ")":
                tokens = tokens[1:]
            return expr, tokens

        ident = token
        tokens = tokens[1:]
        ident_clean = ident.strip('"').strip('`')

        if not tokens:
            return SQLParser._resolve_column(ident_clean, schema), tokens

        op = tokens[0].upper()

        if op == "IS":
            tokens = tokens[1:]
            if tokens and tokens[0].upper() == "NULL":
                tokens = tokens[1:]
                col_expr = SQLParser._resolve_column(ident_clean, schema)
                return col_expr.is_null() | (col_expr.cast(pl.Utf8, strict=False) == pl.lit("NA")) | (col_expr.cast(pl.Utf8, strict=False) == pl.lit("")), tokens
            raise ValueError(f"Unexpected token after IS: {tokens}")

        elif op == "IS NOT":
            tokens = tokens[1:]
            if tokens and tokens[0].upper() == "NULL":
                tokens = tokens[1:]
                col_expr = SQLParser._resolve_column(ident_clean, schema)
                return col_expr.is_not_null() & (col_expr.cast(pl.Utf8, strict=False) != pl.lit("NA")) & (col_expr.cast(pl.Utf8, strict=False) != pl.lit("")), tokens
            raise ValueError(f"Unexpected token after IS NOT: {tokens}")

        elif op in ("=", "==", "!=", "<>", "<", "<=", ">", ">="):
            tokens = tokens[1:]
            if not tokens:
                raise ValueError(f"Missing operand after {op}")
            raw_val = tokens[0]
            tokens = tokens[1:]

            val = SQLParser._parse_literal(raw_val)
            col_expr = SQLParser._resolve_column(ident_clean, schema)
            return SQLParser._build_comparison(col_expr, op, val, schema, ident_clean), tokens

        elif op == "IN":
            tokens = tokens[1:]
            if tokens and tokens[0] == "(":
                tokens = tokens[1:]
                in_values = []
                while tokens and tokens[0] != ")":
                    if tokens[0] != ",":
                        in_values.append(SQLParser._parse_literal(tokens[0]))
                    tokens = tokens[1:]
                if tokens and tokens[0] == ")":
                    tokens = tokens[1:]
                col_expr = SQLParser._resolve_column(ident_clean, schema)
                # Check type
                if in_values and all(isinstance(v, (int, float)) for v in in_values):
                    return col_expr.cast(pl.Float64, strict=False).is_in([float(v) for v in in_values]), tokens
                return col_expr.cast(pl.Utf8, strict=False).is_in([str(v) for v in in_values]), tokens
            raise ValueError("Expected '(' after IN")

        elif op == "NOT IN":
            tokens = tokens[1:]
            if tokens and tokens[0] == "(":
                tokens = tokens[1:]
                in_values = []
                while tokens and tokens[0] != ")":
                    if tokens[0] != ",":
                        in_values.append(SQLParser._parse_literal(tokens[0]))
                    tokens = tokens[1:]
                if tokens and tokens[0] == ")":
                    tokens = tokens[1:]
                col_expr = SQLParser._resolve_column(ident_clean, schema)
                if in_values and all(isinstance(v, (int, float)) for v in in_values):
                    return ~col_expr.cast(pl.Float64, strict=False).is_in([float(v) for v in in_values]), tokens
                return ~col_expr.cast(pl.Utf8, strict=False).is_in([str(v) for v in in_values]), tokens
            raise ValueError("Expected '(' after NOT IN")

        elif op == "LIKE":
            tokens = tokens[1:]
            pattern = SQLParser._parse_literal(tokens[0])
            tokens = tokens[1:]
            regex_pat = "^" + re.escape(str(pattern)).replace("%", ".*").replace("_", ".") + "$"
            col_expr = SQLParser._resolve_column(ident_clean, schema).cast(pl.Utf8, strict=False)
            return col_expr.str.contains(regex_pat), tokens

        return SQLParser._resolve_column(ident_clean, schema), tokens

    @staticmethod
    def _resolve_column(col_name: str, schema: Optional[dict]) -> pl.Expr:
        if schema:
            if col_name in schema:
                return pl.col(col_name)
            if "." in col_name:
                short_name = col_name.split(".", 1)[1]
                if short_name in schema:
                    return pl.col(short_name)
        if "." in col_name:
            return pl.col(col_name.split(".", 1)[1])
        return pl.col(col_name)

    @staticmethod
    def _parse_literal(raw_val: str) -> Any:
        if (raw_val.startswith("'") and raw_val.endswith("'")) or (raw_val.startswith('"') and raw_val.endswith('"')):
            return raw_val[1:-1]
        if raw_val.upper() == "NULL":
            return None
        if raw_val.upper() == "TRUE":
            return True
        if raw_val.upper() == "FALSE":
            return False
        try:
            if "." in raw_val:
                return float(raw_val)
            return int(raw_val)
        except ValueError:
            return raw_val

    @staticmethod
    def _build_comparison(
        col_expr: pl.Expr,
        op: str,
        val: Any,
        schema: Optional[dict],
        col_name: str
    ) -> pl.Expr:
        if op in ("=", "=="):
            if val is None:
                return col_expr.is_null()
            if isinstance(val, (int, float)):
                return col_expr.cast(pl.Float64, strict=False) == pl.lit(float(val))
            return col_expr.cast(pl.Utf8, strict=False) == pl.lit(str(val))

        elif op in ("!=", "<>"):
            if val is None:
                return col_expr.is_not_null()
            if isinstance(val, (int, float)):
                return col_expr.cast(pl.Float64, strict=False) != pl.lit(float(val))
            return col_expr.cast(pl.Utf8, strict=False) != pl.lit(str(val))

        elif op == "<":
            if isinstance(val, (int, float)):
                return col_expr.cast(pl.Float64, strict=False) < pl.lit(float(val))
            return col_expr.cast(pl.Utf8, strict=False) < pl.lit(str(val))

        elif op == "<=":
            if isinstance(val, (int, float)):
                return col_expr.cast(pl.Float64, strict=False) <= pl.lit(float(val))
            return col_expr.cast(pl.Utf8, strict=False) <= pl.lit(str(val))

        elif op == ">":
            if isinstance(val, (int, float)):
                return col_expr.cast(pl.Float64, strict=False) > pl.lit(float(val))
            return col_expr.cast(pl.Utf8, strict=False) > pl.lit(str(val))

        elif op == ">=":
            if isinstance(val, (int, float)):
                return col_expr.cast(pl.Float64, strict=False) >= pl.lit(float(val))
            return col_expr.cast(pl.Utf8, strict=False) >= pl.lit(str(val))

        raise ValueError(f"Unsupported comparison operator: {op}")
