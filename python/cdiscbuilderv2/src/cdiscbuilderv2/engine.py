"""
Unified CDISC Builder v2 Engine.
Parses Yamaa specifications (both generic schemas and domain-mapping schemas),
orchestrates transformations using Polars, and evaluates derivations and verifications.
"""

import logging
from pathlib import Path
from typing import Any, Dict, List, Optional, Union
import polars as pl
import yaml

from .expressions import ExpressionParser
from .sql_parser import SQLParser
from .verifications import VerificationEngine, VerificationReport

logger = logging.getLogger(__name__)


class CDISCEngine:
    """
    Unified Engine for CDISC Builder v2.
    Executes domain transformations with Polars.
    """

    def __init__(
        self,
        spec: Union[str, Path, Dict[str, Any]],
        datasets: Optional[Dict[str, pl.DataFrame]] = None,
        df_long: Optional[pl.DataFrame] = None,
        built_domains: Optional[Dict[str, pl.DataFrame]] = None,
        custom_functions: Optional[Dict[str, Any]] = None
    ):
        self.spec = self._normalize_spec(spec)
        self.datasets: Dict[str, pl.DataFrame] = datasets or {}
        self.df_long: Optional[pl.DataFrame] = df_long
        self.built_domains: Dict[str, pl.DataFrame] = built_domains or {}
        self.custom_functions = custom_functions or {}
        self.target_df: Optional[pl.DataFrame] = None
        self.verification_report: Optional[VerificationReport] = None

    def _normalize_spec(self, spec_input: Union[str, Path, Dict[str, Any]]) -> Dict[str, Any]:
        """Load and normalize spec if path or domain-keyed dict is passed."""
        if isinstance(spec_input, (str, Path)):
            spec_path = Path(spec_input)
            with open(spec_path, "r") as f:
                data = yaml.safe_load(f)
        else:
            data = spec_input

        if isinstance(data, dict) and "domain" not in data and len(data) == 1:
            domain_name = list(data.keys())[0]
            domain_content = data[domain_name]
            return {
                "domain": domain_name,
                "is_domain_format": True,
                "raw_config": domain_content
            }

        return data

    def load_datasets(self) -> None:
        """Load datasets configured in 'datasets' section if not already present in memory."""
        dataset_config = self.spec.get("datasets", {})
        for alias, path_str in dataset_config.items():
            if alias in self.datasets:
                continue

            path = Path(path_str)
            if not path.exists():
                logger.warning(f"Dataset '{alias}' path does not exist: {path}")
                continue

            if path.suffix == ".parquet":
                df = pl.read_parquet(path)
            elif path.suffix == ".csv":
                df = pl.read_csv(path)
            else:
                logger.warning(f"Unsupported format for dataset '{alias}': {path}")
                continue

            self.datasets[alias] = df
            logger.info(f"Loaded dataset '{alias}' with shape {df.shape}")

    def _pivot_long_df(self, form_oids: Optional[List[str]], keys: Optional[List[str]]) -> pl.DataFrame:
        """Helper to filter and pivot long data by FormOID and ItemOID."""
        if self.df_long is None or self.df_long.height == 0:
            return pl.DataFrame()

        src = self.df_long
        if form_oids:
            src = src.filter(pl.col("FormOID").is_in(form_oids))

        if src.height == 0:
            return pl.DataFrame()

        default_keys = ["StudyOID", "SubjectKey", "StudyEventOID", "ItemGroupRepeatKey"]
        use_keys = keys or default_keys
        valid_keys = [k for k in use_keys if k in src.columns]

        pivoted = src.pivot(
            on="ItemOID",
            index=valid_keys,
            values="Value",
            aggregate_function="first"
        )
        return pivoted

    def _build_findings_domain(self, domain_name: str, config: Dict[str, Any]) -> pl.DataFrame:
        """Build Findings-class domain (e.g. LB, VS, MB, EG) with stacked test measurements."""
        findings_cols = config.get("columns", [])
        definitions = config.get("definitions", {})
        sub_dfs: List[pl.DataFrame] = []
        all_finding_col_names: List[str] = []

        for finding in findings_cols:
            form_oids = finding.get("formoid", [])
            keys = finding.get("keys", ["StudyOID", "SubjectKey", "StudyEventOID", "ItemGroupRepeatKey"])
            
            pivoted = self._pivot_long_df(form_oids, keys)
            if pivoted.height == 0:
                continue

            reserved = {"name", "type", "formoid", "itemoid", "keys"}
            col_mappings = {k: v for k, v in finding.items() if k not in reserved}
            for k in col_mappings.keys():
                if k not in all_finding_col_names:
                    all_finding_col_names.append(k)

            mapped_df = self._apply_column_mappings_to_df(pivoted, col_mappings, retain_keys=True)
            sub_dfs.append(mapped_df)

        if not sub_dfs:
            return pl.DataFrame()

        combined = pl.concat(sub_dfs, how="diagonal")
        if combined.height == 0:
            return pl.DataFrame()

        if definitions:
            combined = self._apply_column_mappings_to_df(combined, definitions, retain_keys=False, extra_cols_to_keep=all_finding_col_names)

        final_cols = []
        for c in list(definitions.keys()) + all_finding_col_names:
            if c in combined.columns and c not in final_cols:
                final_cols.append(c)

        if final_cols:
            combined = combined.select(final_cols)

        return combined

    def _build_event_domain(self, domain_name: str, sources: List[Dict[str, Any]]) -> pl.DataFrame:
        """Build Event or Intervention domain (e.g. AE, CM, EX, DS, MH, DM)."""
        domain_dfs: List[pl.DataFrame] = []

        for src_cfg in sources:
            if src_cfg.get("supp"):
                continue

            form_oids = src_cfg.get("formoid", [])
            keys = src_cfg.get("keys", ["StudyOID", "SubjectKey", "StudyEventOID", "ItemGroupRepeatKey"])

            pivoted = self._pivot_long_df(form_oids, keys)
            if pivoted.height == 0:
                continue

            col_mappings = src_cfg.get("columns", {})
            mapped_df = self._apply_column_mappings_to_df(pivoted, col_mappings, retain_keys=False)
            domain_dfs.append(mapped_df)

        if not domain_dfs:
            return pl.DataFrame()

        combined = pl.concat(domain_dfs, how="diagonal") if len(domain_dfs) > 1 else domain_dfs[0]
        return combined

    def _apply_column_mappings_to_df(
        self,
        df: pl.DataFrame,
        mappings: Dict[str, Any],
        retain_keys: bool = False,
        extra_cols_to_keep: Optional[List[str]] = None
    ) -> pl.DataFrame:
        """Apply a set of column derivation rules to a DataFrame."""
        parser = ExpressionParser(self._get_all_context_datasets(), self.custom_functions)
        
        standard_cols: Dict[str, Any] = {}
        grouped_cols: Dict[str, Any] = {}

        for col_name, col_cfg in mappings.items():
            if isinstance(col_cfg, dict) and "group" in col_cfg:
                grouped_cols[col_name] = col_cfg
            else:
                standard_cols[col_name] = col_cfg

        # 1. Standard columns
        for col_name, col_cfg in standard_cols.items():
            col_expr = self._resolve_column_expression(df, col_name, col_cfg, parser)
            if col_expr is not None:
                df = df.with_columns(col_expr.alias(col_name))

        # 2. Grouped sequence columns (--SEQ)
        for col_name, col_cfg in grouped_cols.items():
            grp = col_cfg.get("group", [])
            if isinstance(grp, str):
                grp = [grp]

            valid_grp = [c for c in grp if c in df.columns]
            seq_expr = pl.int_range(1, pl.len() + 1)
            if valid_grp:
                seq_expr = seq_expr.over(valid_grp)

            df = df.with_columns(seq_expr.alias(col_name))

        if retain_keys:
            key_cols = ["StudyOID", "SubjectKey", "StudyEventOID", "ItemGroupRepeatKey"]
            keep = [k for k in key_cols if k in df.columns] + list(mappings.keys())
            valid_keep = [c for c in keep if c in df.columns]
            return df.select(valid_keep)

        target_cols = list(mappings.keys())
        if extra_cols_to_keep:
            target_cols = list(mappings.keys()) + [c for c in extra_cols_to_keep if c not in mappings]

        valid_targets = [c for c in target_cols if c in df.columns]
        return df.select(valid_targets)

    def _resolve_column_expression(
        self,
        df: pl.DataFrame,
        col_name: str,
        col_cfg: Any,
        parser: ExpressionParser
    ) -> Optional[pl.Expr]:
        """Convert a column config into a Polars expression."""
        if col_cfg is None:
            return pl.lit(None)

        if isinstance(col_cfg, str):
            col_cfg = {"source": col_cfg}

        if not isinstance(col_cfg, dict):
            return pl.lit(col_cfg)

        schema = {c: None for c in df.columns}

        # Cross-domain reference in source (e.g. _DM_REF.RFSTDTC or EX.EXSTDTC or MH.MHSEQ)
        if "source" in col_cfg:
            src = col_cfg["source"]
            if isinstance(src, str) and "." in src and not src.startswith("IT."):
                parts = src.split(".", 1)
                ref_domain, ref_col = parts[0], parts[1]
                
                subj_col = "USUBJID" if "USUBJID" in df.columns else ("SubjectKey" if "SubjectKey" in df.columns else None)
                
                if ref_domain == "_DM_REF" and ref_domain not in self.built_domains and "DM" in self.built_domains:
                    ref_domain = "DM"

                if ref_domain in self.built_domains and subj_col:
                    ref_df = self.built_domains[ref_domain]
                    ref_key = "USUBJID" if "USUBJID" in ref_df.columns else ("SubjectKey" if "SubjectKey" in ref_df.columns else None)
                    if ref_col in ref_df.columns and ref_key:
                        unique_ref = ref_df.select([ref_key, ref_col]).unique(subset=[ref_key])
                        keys = [str(k) for k in unique_ref[ref_key].to_list()]
                        vals = [str(v) if v is not None else "" for v in unique_ref[ref_col].to_list()]
                        
                        res_expr = pl.col(subj_col).cast(pl.Utf8, strict=False).replace_strict(keys, vals, default=None, return_dtype=pl.Utf8)
                        if col_cfg.get("type") == "float":
                            res_expr = res_expr.cast(pl.Float64, strict=False)
                        elif col_cfg.get("type") == "int":
                            res_expr = res_expr.cast(pl.Int64, strict=False)
                        return res_expr
                return pl.lit(None)

            if isinstance(src, str) and src not in df.columns:
                if "fallback" in col_cfg and col_cfg["fallback"] in df.columns:
                    src = col_cfg["fallback"]
                else:
                    return pl.lit(None)

            if "value_mapping" in col_cfg:
                return parser.parse({
                    "mapping": {
                        "source": src,
                        "dict": col_cfg["value_mapping"],
                        "case_sensitive": col_cfg.get("case_sensitive", True),
                        "missing": col_cfg.get("fallback")
                    }
                }, schema)

            if "fallback" in col_cfg:
                return parser.parse({
                    "coalesce": {
                        "sources": [src, col_cfg["fallback"]]
                    }
                }, schema)

            expr = parser.parse({"source": src}, schema)
            if col_cfg.get("type") == "float":
                expr = expr.cast(pl.Float64, strict=False)
            elif col_cfg.get("type") == "int":
                expr = expr.cast(pl.Int64, strict=False)
            return expr

        if "literal" in col_cfg:
            return pl.lit(col_cfg["literal"])

        if "function_" in col_cfg or "function" in col_cfg:
            fn_name = col_cfg.get("function_", col_cfg.get("function"))
            args = col_cfg.get("args", [])
            
            if fn_name in ("calculate_study_day", "calc_study_day") and len(args) == 2:
                dtc_col = args[0]
                ref_arg = args[1]
                
                dtc_expr = pl.col(dtc_col) if dtc_col in df.columns else pl.lit(None)
                
                subj_col = "USUBJID" if "USUBJID" in df.columns else ("SubjectKey" if "SubjectKey" in df.columns else None)
                if "." in ref_arg:
                    r_domain, r_col = ref_arg.split(".", 1)
                    if r_domain in self.built_domains and subj_col:
                        r_df = self.built_domains[r_domain]
                        r_key = "USUBJID" if "USUBJID" in r_df.columns else ("SubjectKey" if "SubjectKey" in r_df.columns else None)
                        if r_col in r_df.columns and r_key:
                            unique_r = r_df.select([r_key, r_col]).unique(subset=[r_key])
                            keys = [str(k) for k in unique_r[r_key].to_list()]
                            vals = [str(v) if v is not None else "" for v in unique_r[r_col].to_list()]
                            ref_expr = pl.col(subj_col).cast(pl.Utf8, strict=False).replace_strict(keys, vals, default=None, return_dtype=pl.Utf8)
                        else:
                            ref_expr = pl.lit(None)
                    else:
                        ref_expr = pl.lit(None)
                else:
                    ref_expr = pl.col(ref_arg) if ref_arg in df.columns else pl.lit(None)

                d_date = dtc_expr.cast(pl.Utf8, strict=False).str.slice(0, 10).str.to_date("%Y-%m-%d", strict=False)
                r_date = ref_expr.cast(pl.Utf8, strict=False).str.slice(0, 10).str.to_date("%Y-%m-%d", strict=False)
                days = (d_date - r_date).dt.total_days()
                return pl.when(days >= 0).then(days + 1).otherwise(days).cast(pl.Int32, strict=False)

            return parser.parse({"function": {"name": fn_name, "args": args}}, schema)

        if "derivation" in col_cfg:
            return parser.parse(col_cfg["derivation"], schema)

        return parser.parse(col_cfg, schema)

    def _get_all_context_datasets(self) -> Dict[str, pl.DataFrame]:
        """Combine raw datasets, long dataframe, and built domains into a unified map."""
        all_ds = {}
        all_ds.update(self.datasets)
        all_ds.update(self.built_domains)
        if self.df_long is not None:
            all_ds["ODM"] = self.df_long
        return all_ds

    def build_generic(self) -> pl.DataFrame:
        """Build standard generic schema dataset."""
        base_alias = self.spec.get("base")
        parents = self.spec.get("parents", [])
        if isinstance(parents, str):
            parents = [parents]
        keys = self.spec.get("keys", [])

        # FormOID based pivoting on raw ODM (e.g. DM, Events)
        form_oids = self.spec.get("formoid") or self.spec.get("form_oids")
        if form_oids and (base_alias == "ODM" or self.df_long is not None):
            if isinstance(form_oids, str):
                form_oids = [form_oids]
            pivot_keys = self.spec.get("pivot_keys", ["StudyOID", "SubjectKey", "StudyEventOID", "ItemGroupRepeatKey"])
            base_df = self._pivot_long_df(form_oids, pivot_keys)
        elif base_alias and base_alias in self.datasets:
            base_df = self.datasets[base_alias].clone()
        elif base_alias == "ODM" and self.df_long is not None:
            base_df = self.df_long.clone()
        elif parents:
            parent_dfs = []
            for p in parents:
                path = Path(p)
                alias = path.stem
                if alias in self.datasets:
                    parent_dfs.append(self.datasets[alias])
                elif path.exists():
                    df = pl.read_parquet(path) if path.suffix == ".parquet" else pl.read_csv(path)
                    self.datasets[alias] = df
                    parent_dfs.append(df)

            if parent_dfs:
                base_df = parent_dfs[0].clone()
                for idx, pdf in enumerate(parent_dfs[1:]):
                    join_keys = [k for k in keys if k in base_df.columns and k in pdf.columns]
                    if join_keys:
                        base_df = base_df.join(pdf, on=join_keys, how="left", suffix=f"_p{idx+1}")
            else:
                base_df = pl.DataFrame()
        elif "ODM" in self._get_all_context_datasets():
            base_df = self._get_all_context_datasets()["ODM"].clone()
        else:
            base_df = pl.DataFrame()

        rows_spec = self.spec.get("rows", [])
        
        # Check if rows filtering is used on an ODM item dataset (e.g. Findings or Multi-source filtering)
        is_item_sliced = (
            base_alias == "ODM" or "ItemOID" in base_df.columns
        ) and rows_spec and len(rows_spec) > 0 and any(r.get("filter") for r in rows_spec)

        if is_item_sliced and base_df.height > 0:
            sub_dfs = []
            parser = ExpressionParser(self._get_all_context_datasets(), self.custom_functions)

            for row_item in rows_spec:
                filter_sql = row_item.get("filter")
                derivations = row_item.get("derivations", {})

                sliced_df = base_df
                if filter_sql:
                    schema_b = {c: None for c in base_df.columns}
                    f_expr = SQLParser.parse_to_expr(filter_sql, schema_b)
                    sliced_df = sliced_df.filter(f_expr)

                if sliced_df.height == 0:
                    continue

                schema_s = {c: None for c in sliced_df.columns}
                for col_name, derivation in derivations.items():
                    val_expr = parser.parse(derivation, schema_s)
                    sliced_df = sliced_df.with_columns(val_expr.alias(col_name))
                    schema_s[col_name] = None

                sub_dfs.append(sliced_df)

            if sub_dfs:
                self.target_df = pl.concat(sub_dfs, how="diagonal") if len(sub_dfs) > 1 else sub_dfs[0]
            else:
                self.target_df = pl.DataFrame()

            # Apply top-level column derivations
            self.apply_generic_columns()
        else:
            self.target_df = base_df
            # 1. Apply column derivations first
            self.apply_generic_columns()
            # 2. Apply row-level overrides
            self.apply_row_filters()

        # Finally select specified columns if requested
        columns_spec = self.spec.get("columns", [])
        if columns_spec:
            target_names = [c.get("name") for c in columns_spec if c.get("name")]
            valid_cols = [c for c in target_names if c in self.target_df.columns]
            if valid_cols:
                self.target_df = self.target_df.select(valid_cols)

        return self.target_df

    def apply_generic_columns(self) -> None:
        """Process 'columns' block for generic schema."""
        columns_spec = self.spec.get("columns", [])
        if not columns_spec or self.target_df is None or self.target_df.height == 0:
            return

        parser = ExpressionParser(self._get_all_context_datasets(), self.custom_functions)
        schema = {c: None for c in self.target_df.columns}

        for col_spec in columns_spec:
            col_name = col_spec.get("name")
            derivation = col_spec.get("derivation")

            if derivation:
                expr = parser.parse(derivation, schema)
                if expr is not None:
                    if "type" in col_spec:
                        col_type = col_spec["type"]
                        if col_type == "int":
                            expr = expr.cast(pl.Int64, strict=False)
                        elif col_type == "float":
                            expr = expr.cast(pl.Float64, strict=False)
                        elif col_type == "bool":
                            expr = expr.cast(pl.Boolean, strict=False)
                        elif col_type == "date":
                            expr = expr.cast(pl.Utf8, strict=False).str.slice(0, 10)
                        elif col_type == "str":
                            expr = expr.cast(pl.Utf8, strict=False)

                    self.target_df = self.target_df.with_columns(expr.alias(col_name))
                    schema[col_name] = None
            else:
                if col_name not in self.target_df.columns:
                    self.target_df = self.target_df.with_columns(pl.lit(None).alias(col_name))
                    schema[col_name] = None

    def apply_row_filters(self) -> None:
        """Process 'rows' block for row-level derivations and overrides."""
        rows_spec = self.spec.get("rows", [])
        if not rows_spec or self.target_df is None or self.target_df.height == 0:
            return

        parser = ExpressionParser(self._get_all_context_datasets(), self.custom_functions)
        schema = {c: None for c in self.target_df.columns}

        for row_spec in rows_spec:
            filter_sql = row_spec.get("filter")
            derivations = row_spec.get("derivations", {})

            if not derivations:
                continue

            filter_expr = SQLParser.parse_to_expr(filter_sql, schema) if filter_sql else pl.lit(True)

            for col_name, derivation in derivations.items():
                val_expr = parser.parse(derivation, schema)
                if col_name not in self.target_df.columns:
                    self.target_df = self.target_df.with_columns(pl.lit(None).alias(col_name))

                existing_type = self.target_df.schema.get(col_name)
                if existing_type in (pl.Float64, pl.Float32):
                    val_expr = val_expr.cast(pl.Float64, strict=False)
                elif existing_type in (pl.Int64, pl.Int32):
                    val_expr = val_expr.cast(pl.Int64, strict=False)

                expr = pl.when(filter_expr).then(val_expr).otherwise(pl.col(col_name))
                self.target_df = self.target_df.with_columns(expr.alias(col_name))

    def build(self) -> pl.DataFrame:
        """Main build execution."""
        domain = self.spec.get("domain", "DATASET")
        logger.info(f"Building domain: {domain}")

        if self.spec.get("is_domain_format"):
            raw_cfg = self.spec.get("raw_config")
            if isinstance(raw_cfg, dict) and raw_cfg.get("type") == "FINDINGS":
                self.target_df = self._build_findings_domain(domain, raw_cfg)
            elif isinstance(raw_cfg, list):
                self.target_df = self._build_event_domain(domain, raw_cfg)
            elif isinstance(raw_cfg, dict):
                self.target_df = self._build_event_domain(domain, [raw_cfg])
            else:
                self.target_df = pl.DataFrame()
        else:
            self.load_datasets()
            self.target_df = self.build_generic()

        self.verification_report = VerificationEngine.verify(self.target_df, self.spec)
        return self.target_df

    def save(self, output_dir: Union[str, Path], formats: Optional[List[str]] = None) -> Dict[str, Path]:
        """Save the target DataFrame to Parquet, CSV, or SAS XPT."""
        if self.target_df is None:
            raise ValueError("Target DataFrame is not built. Call build() first.")

        domain = self.spec.get("domain", "unknown").lower()
        out_p = Path(output_dir)
        out_p.mkdir(parents=True, exist_ok=True)

        if not formats:
            formats = ["parquet", "csv"]

        saved_files = {}

        if "parquet" in formats:
            pq_path = out_p / f"{domain}.parquet"
            self.target_df.write_parquet(pq_path)
            saved_files["parquet"] = pq_path

        if "csv" in formats:
            csv_path = out_p / f"{domain}.csv"
            self.target_df.write_csv(csv_path)
            saved_files["csv"] = csv_path

        if "xpt" in formats:
            try:
                import pyreadstat
                xpt_path = out_p / f"{domain}.xpt"
                pdf = self.target_df.to_pandas()
                pyreadstat.write_xport(pdf, str(xpt_path), table_name=domain.upper())
                saved_files["xpt"] = xpt_path
            except Exception as e:
                logger.warning(f"Error exporting XPT for {domain}: {e}")

        return saved_files
