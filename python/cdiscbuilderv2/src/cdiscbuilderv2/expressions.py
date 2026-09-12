"""
Expression parser for translating Yamaa declarative schemas into Polars expressions.
Supports core, string, numeric, date, window, aggregate, mapping, and custom functions.
"""

import re
import polars as pl
from typing import Any, Dict, List, Optional, Union
from .sql_parser import SQLParser


class ExpressionParser:
    """
    Parses yamaa expression schemas and translates them into Polars expressions.
    """

    def __init__(self, datasets: Optional[Dict[str, pl.DataFrame]] = None, custom_functions: Optional[Dict[str, Any]] = None):
        self.datasets = datasets or {}
        self.custom_functions = custom_functions or {}

    def parse(self, derivation_dict: Any, schema: Optional[dict] = None) -> pl.Expr:
        """
        Main entry point for parsing a derivation dictionary into a Polars Expression.
        """
        if derivation_dict is None:
            return pl.lit(None)

        if not isinstance(derivation_dict, dict):
            return self._parse_expression(derivation_dict, schema)

        # Handle handled_expression_class
        if "value" in derivation_dict:
            expr = self._parse_expression(derivation_dict["value"], schema)

            # Handle conversion_failure imputation
            if "conversion_failure" in derivation_dict:
                expr = expr.fill_null(pl.lit(derivation_dict["conversion_failure"]))

            # Handle override rules
            overrides = derivation_dict.get("override", [])
            if overrides:
                when_expr = SQLParser.parse_to_expr(overrides[0]["when"], schema)
                val_expr = self._parse_expression(overrides[0]["value"], schema)
                case_chain = pl.when(when_expr).then(val_expr)

                for override in overrides[1:]:
                    w_expr = SQLParser.parse_to_expr(override["when"], schema)
                    v_expr = self._parse_expression(override["value"], schema)
                    case_chain = case_chain.when(w_expr).then(v_expr)

                expr = case_chain.otherwise(expr)

            return expr

        return self._parse_expression(derivation_dict, schema)

    def _parse_expression(self, expr_def: Any, schema: Optional[dict] = None) -> pl.Expr:
        if expr_def is None:
            return pl.lit(None)

        if isinstance(expr_def, str):
            if (expr_def.startswith("'") and expr_def.endswith("'")) or (expr_def.startswith('"') and expr_def.endswith('"')):
                return pl.lit(expr_def[1:-1])

            if schema is not None:
                if expr_def in schema:
                    return pl.col(expr_def)
                if "." in expr_def:
                    parts = expr_def.split(".", 1)
                    if parts[0] in self.datasets:
                        return pl.col(parts[1]) if parts[1] in schema else pl.col(expr_def)
                if not re.match(r"^[A-Za-z_][A-Za-z0-9_.]*$", expr_def):
                    return pl.lit(expr_def)
                return pl.col(expr_def)

            if not re.match(r"^[A-Za-z_][A-Za-z0-9_.]*$", expr_def):
                return pl.lit(expr_def)
            return pl.col(expr_def)

        if isinstance(expr_def, (int, float, bool)):
            return pl.lit(expr_def)

        if not isinstance(expr_def, dict):
            return pl.lit(None)

        key = list(expr_def.keys())[0]
        value = expr_def[key]

        # --- Core Expressions ---
        if key == "source":
            if isinstance(value, str):
                if schema is not None and value not in schema:
                    if "." in value:
                        parts = value.split(".", 1)
                        if parts[1] in schema:
                            return pl.col(parts[1])
                    return pl.lit(None)
                return pl.col(value)

            if isinstance(value, dict):
                var = value["variable"]
                if schema is not None and var not in schema:
                    return pl.lit(value.get("missing", None))
                col = pl.col(var)
                if "missing" in value and value["missing"] is not None:
                    col = col.fill_null(pl.lit(value["missing"]))
                return col
            return pl.lit(None)

        if key == "literal":
            return pl.lit(value)

        if key == "coalesce":
            sources = [self._parse_expression(s, schema) for s in value.get("sources", [])]
            if not sources:
                return pl.lit(None)
            expr = pl.coalesce(sources)
            if "default" in value and value["default"] is not None:
                expr = expr.fill_null(pl.lit(value["default"]))
            return expr

        if key == "case":
            branches = value.get("branches", [])
            if not branches:
                return pl.lit(None)

            when_expr = SQLParser.parse_to_expr(branches[0]["when"], schema)
            then_expr = self._parse_expression(branches[0]["then"], schema)
            chain = pl.when(when_expr).then(then_expr)

            for branch in branches[1:]:
                w_expr = SQLParser.parse_to_expr(branch["when"], schema)
                t_expr = self._parse_expression(branch["then"], schema)
                chain = chain.when(w_expr).then(t_expr)

            if "else" in value:
                return chain.otherwise(self._parse_expression(value["else"], schema))
            return chain.otherwise(pl.lit(None))

        # --- String Expressions ---
        if key == "str_concat":
            sources = [self._parse_expression(s, schema) for s in value.get("sources", [])]
            if not sources:
                return pl.lit(None)
            expr = sources[0].cast(pl.Utf8, strict=False).fill_null("")
            for e in sources[1:]:
                expr = expr + e.cast(pl.Utf8, strict=False).fill_null("")
            if "missing" in value and value["missing"] is not None:
                expr = pl.when(expr == "").then(pl.lit(value["missing"])).otherwise(expr)
            return expr

        if key == "str_upper":
            source_arg = value.get("source", value) if isinstance(value, dict) else value
            expr = self._parse_expression(source_arg, schema).cast(pl.Utf8, strict=False).str.to_uppercase()
            if isinstance(value, dict) and "missing" in value and value["missing"] is not None:
                expr = expr.fill_null(pl.lit(value["missing"]))
            return expr

        if key == "str_lower":
            source_arg = value.get("source", value) if isinstance(value, dict) else value
            expr = self._parse_expression(source_arg, schema).cast(pl.Utf8, strict=False).str.to_lowercase()
            if isinstance(value, dict) and "missing" in value and value["missing"] is not None:
                expr = expr.fill_null(pl.lit(value["missing"]))
            return expr

        if key == "str_extract":
            source_col = value["source"]
            expr = self._parse_expression(source_col, schema).cast(pl.Utf8, strict=False)
            pattern = value["pattern"]
            group = value.get("group", 0)
            expr = expr.str.extract(pattern, group)
            if "no_match" in value and value["no_match"] is not None:
                expr = expr.fill_null(pl.lit(value["no_match"]))
            if "missing" in value and value["missing"] is not None:
                expr = expr.fill_null(pl.lit(value["missing"]))
            return expr

        # --- Numeric Expressions ---
        if key == "multiply":
            return self._parse_expression(value["source"], schema).cast(pl.Float64, strict=False) * float(value["factor"])

        if key == "add":
            return self._parse_expression(value["source"], schema).cast(pl.Float64, strict=False) + float(value["addend"])

        if key == "subtract":
            minuend = self._parse_expression(value["minuend"], schema).cast(pl.Float64, strict=False)
            subtrahend = self._parse_expression(value["subtrahend"], schema).cast(pl.Float64, strict=False)
            return minuend - subtrahend

        if key == "percent_change":
            val = self._parse_expression(value["value"], schema).cast(pl.Float64, strict=False)
            base = self._parse_expression(value["base"], schema).cast(pl.Float64, strict=False)
            return ((val - base) / base) * 100.0

        if key == "round":
            src = self._parse_expression(value["source"], schema).cast(pl.Float64, strict=False)
            decimals = value.get("decimals", 0)
            return src.round(decimals)

        # --- Date Expressions ---
        if key == "date_diff":
            start_col = self._parse_expression(value["start"], schema)
            end_col = self._parse_expression(value["end"], schema)
            unit = value.get("unit", "day").lower()

            start_date = start_col.cast(pl.Date, strict=False)
            end_date = end_col.cast(pl.Date, strict=False)
            diff_days = (end_date - start_date).dt.total_days()

            if unit == "day":
                return diff_days
            elif unit == "week":
                return diff_days / 7.0
            elif unit == "month":
                return diff_days / 30.4375
            elif unit == "year":
                return diff_days / 365.25
            return diff_days

        if key in ("calculate_study_day", "calc_study_day"):
            dtc = self._parse_expression(value.get("date", value.get("dtc", value.get("args", [None])[0])), schema)
            ref_dtc = self._parse_expression(value.get("reference_date", value.get("ref_date", value.get("args", [None, None])[1])), schema)
            
            d_date = dtc.cast(pl.Utf8, strict=False).str.slice(0, 10).str.to_date("%Y-%m-%d", strict=False)
            r_date = ref_dtc.cast(pl.Utf8, strict=False).str.slice(0, 10).str.to_date("%Y-%m-%d", strict=False)
            days = (d_date - r_date).dt.total_days()
            
            study_day = pl.when(days >= 0).then(days + 1).otherwise(days)
            return study_day.cast(pl.Int32, strict=False)

        # --- Window Expressions ---
        if key == "row_number":
            group_by = value.get("group_by", [])
            order_by = value.get("order_by", [])

            if isinstance(group_by, str):
                group_by = [group_by]
            if isinstance(order_by, str):
                order_by = [order_by]

            expr = pl.int_range(1, pl.len() + 1)
            if group_by:
                expr = expr.over(group_by)
            return expr

        if key == "baseline_flag":
            group_by = value.get("group_by", [])
            if isinstance(group_by, str):
                group_by = [group_by]
            date_col = value["date"]
            ref_date = value["reference_date"]

            d = pl.col(date_col).cast(pl.Utf8, strict=False).str.slice(0, 10)
            r = pl.col(ref_date).cast(pl.Utf8, strict=False).str.slice(0, 10)
            
            is_before = d <= r
            max_date = pl.when(is_before).then(d).otherwise(None).max()
            if group_by:
                max_date = max_date.over(group_by)

            expr = pl.when((d == max_date) & is_before).then(pl.lit("Y")).otherwise(pl.lit(None))
            return expr

        if key == "baseline_value":
            group_by = value.get("group_by", [])
            if isinstance(group_by, str):
                group_by = [group_by]
            val_col = value["value"]
            flag_col = value.get("flag", "ABLFL")

            base_val = pl.when(pl.col(flag_col) == "Y").then(pl.col(val_col)).otherwise(None).max()
            if group_by:
                base_val = base_val.over(group_by)
            return base_val

        # --- Aggregate Expressions ---
        if key in ["min", "max", "sum", "mean", "count"]:
            source_expr = self._parse_expression(value["source"], schema)
            if "filter" in value and value["filter"]:
                filter_expr = SQLParser.parse_to_expr(value["filter"], schema)
                source_expr = pl.when(filter_expr).then(source_expr).otherwise(None)

            if key == "min":
                expr = source_expr.min()
            elif key == "max":
                expr = source_expr.max()
            elif key == "sum":
                expr = source_expr.sum()
            elif key == "mean":
                expr = source_expr.mean()
            elif key == "count":
                expr = source_expr.count()

            group_by = value.get("group_by", [])
            if isinstance(group_by, str):
                group_by = [group_by]
            if group_by:
                expr = expr.over(group_by)

            if "missing" in value and value["missing"] is not None:
                expr = expr.fill_null(pl.lit(value["missing"]))
            return expr

        # --- Mapping Expressions ---
        if key == "mapping":
            source = self._parse_expression(value["source"], schema).cast(pl.Utf8, strict=False)
            mapping_dict = {str(k): str(v) for k, v in value["dict"].items()}
            case_sensitive = value.get("case_sensitive", True)
            
            keys = list(mapping_dict.keys())
            vals = list(mapping_dict.values())
            has_unmapped = "unmapped" in value
            default_val = value.get("unmapped", None)
            
            if not case_sensitive:
                lower_keys = [k.lower() for k in keys]
                mapped_expr = source.str.to_lowercase().replace_strict(lower_keys, vals, default=default_val if has_unmapped else None, return_dtype=pl.Utf8)
            else:
                mapped_expr = source.replace_strict(keys, vals, default=default_val if has_unmapped else None, return_dtype=pl.Utf8)

            expr = mapped_expr if has_unmapped else mapped_expr.fill_null(source)

            if "missing" in value and value["missing"] is not None:
                expr = expr.fill_null(pl.lit(value["missing"]))
            return expr

        if key == "mapping_from":
            dataset_name = value["dataset"]
            source_col = value["source"]
            key_col = value["key"]
            val_col = value["value"]

            if dataset_name in self.datasets:
                ds = self.datasets[dataset_name]
                keys = [str(k) for k in ds[key_col].to_list()]
                vals = [str(v) for v in ds[val_col].to_list()]
                default_val = value.get("unmapped", None)

                src_expr = self._parse_expression(source_col, schema).cast(pl.Utf8, strict=False)
                expr = src_expr.replace_strict(keys, vals, default=default_val, return_dtype=pl.Utf8)
                if "missing" in value and value["missing"] is not None:
                    expr = expr.fill_null(pl.lit(value["missing"]))
                return expr
            return pl.lit(value.get("unmapped", None))

        if key == "cut":
            source = self._parse_expression(value["source"], schema).cast(pl.Float64, strict=False)
            breaks = value["breaks"]
            labels = value["labels"]
            right = value.get("right", False)

            if len(breaks) - 1 != len(labels):
                raise ValueError(f"cut: breaks length ({len(breaks)}) must be len(labels) + 1 ({len(labels) + 1})")

            chain = None
            for idx, label in enumerate(labels):
                low = breaks[idx]
                high = breaks[idx + 1]
                if right:
                    cond = (source > low) & (source <= high)
                else:
                    cond = (source >= low) & (source < high)

                if chain is None:
                    chain = pl.when(cond).then(pl.lit(label))
                else:
                    chain = chain.when(cond).then(pl.lit(label))

            expr = chain.otherwise(pl.lit(value.get("missing", None)))
            return expr

        # --- Medical Coding Expressions ---
        if key == "meddra_decode":
            src = value.get("source", value) if isinstance(value, dict) else value
            src_expr = self._parse_expression(src, schema).cast(pl.Utf8, strict=False)
            from .medical_coder import MEDDRA_CODER
            return src_expr.map_elements(lambda x: MEDDRA_CODER.code_term(x).pt if x else None, return_dtype=pl.Utf8)

        if key == "meddra_soc":
            src = value.get("source", value) if isinstance(value, dict) else value
            src_expr = self._parse_expression(src, schema).cast(pl.Utf8, strict=False)
            from .medical_coder import MEDDRA_CODER
            return src_expr.map_elements(lambda x: MEDDRA_CODER.code_term(x).soc if x else None, return_dtype=pl.Utf8)

        if key == "meddra_ptcd":
            src = value.get("source", value) if isinstance(value, dict) else value
            src_expr = self._parse_expression(src, schema).cast(pl.Utf8, strict=False)
            from .medical_coder import MEDDRA_CODER
            return src_expr.map_elements(lambda x: MEDDRA_CODER.code_term(x).pt_code if x else None, return_dtype=pl.Utf8)

        if key == "whodrug_decode":
            src = value.get("source", value) if isinstance(value, dict) else value
            src_expr = self._parse_expression(src, schema).cast(pl.Utf8, strict=False)
            from .medical_coder import WHO_DRUG_CODER
            return src_expr.map_elements(lambda x: WHO_DRUG_CODER.code_term(x).preferred_name if x else None, return_dtype=pl.Utf8)

        if key == "whodrug_class":
            src = value.get("source", value) if isinstance(value, dict) else value
            src_expr = self._parse_expression(src, schema).cast(pl.Utf8, strict=False)
            from .medical_coder import WHO_DRUG_CODER
            return src_expr.map_elements(lambda x: WHO_DRUG_CODER.code_term(x).therapeutic_class if x else None, return_dtype=pl.Utf8)

        if key == "whodrug_atc":
            src = value.get("source", value) if isinstance(value, dict) else value
            src_expr = self._parse_expression(src, schema).cast(pl.Utf8, strict=False)
            from .medical_coder import WHO_DRUG_CODER
            return src_expr.map_elements(lambda x: WHO_DRUG_CODER.code_term(x).atc_code if x else None, return_dtype=pl.Utf8)

        # --- Custom Function Calling ---
        if key in ("function", "function_"):
            func_name = value.get("name", value) if isinstance(value, dict) else value
            args = value.get("args", []) if isinstance(value, dict) else []

            if func_name in ("calculate_study_day", "calc_study_day"):
                return self._parse_expression({"calculate_study_day": {"args": args}}, schema)

            if func_name in ("meddra_decode", "meddra_pt"):
                arg = args[0] if args else "AETERM"
                return self._parse_expression({"meddra_decode": {"source": arg}}, schema)

            if func_name in ("meddra_soc", "meddra_bodsys"):
                arg = args[0] if args else "AETERM"
                return self._parse_expression({"meddra_soc": {"source": arg}}, schema)

            if func_name in ("whodrug_decode", "whodrug_pt"):
                arg = args[0] if args else "CMTRT"
                return self._parse_expression({"whodrug_decode": {"source": arg}}, schema)

            if func_name in ("whodrug_class", "whodrug_clas"):
                arg = args[0] if args else "CMTRT"
                return self._parse_expression({"whodrug_class": {"source": arg}}, schema)

            if func_name in self.custom_functions:
                return self.custom_functions[func_name](self, args, schema)

            return pl.lit(None)

        return pl.lit(None)
