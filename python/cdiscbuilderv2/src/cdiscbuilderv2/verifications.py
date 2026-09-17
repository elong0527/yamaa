"""
CDISC verification and data quality validation engine.
Executes column-level and dataset-level constraints defined in Yamaa schemas.
"""

import polars as pl
from typing import Any, Dict, List, Optional
from .sql_parser import SQLParser


class VerificationReport:
    """
    Structured report containing results of CDISC verifications and conformance checks.
    """

    def __init__(self, domain: str):
        self.domain = domain
        self.checks: List[Dict[str, Any]] = []

    def add_result(
        self,
        check_type: str,
        target: str,
        rule_desc: str,
        passed: bool,
        total_records: int,
        failing_records: int = 0,
        sample_failures: Optional[List[Any]] = None,
        rule_id: Optional[str] = None,
        message: Optional[str] = None
    ) -> None:
        self.checks.append({
            "domain": self.domain,
            "rule_id": rule_id or check_type,
            "check_type": check_type,
            "target": target,
            "rule": rule_desc,
            "message": message or (f"{target}: {rule_desc}" if target and not rule_desc.startswith(target) else rule_desc),
            "status": "PASS" if passed else "FAIL",
            "passed": passed,
            "total_records": total_records,
            "failing_records": int(failing_records),
            "sample_failures": sample_failures or []
        })

    @property
    def is_valid(self) -> bool:
        return len(self.checks) > 0 and all(c["passed"] for c in self.checks)

    @property
    def pass_count(self) -> int:
        return sum(1 for c in self.checks if c["passed"])

    @property
    def fail_count(self) -> int:
        return sum(1 for c in self.checks if not c["passed"])

    @property
    def pass_rate(self) -> float:
        total = len(self.checks)
        return (self.pass_count / total) if total > 0 else 1.0

    def to_dict(self) -> Dict[str, Any]:
        total = len(self.checks)
        passed = self.pass_count
        failed = self.fail_count
        pass_rate = (passed / total) if total > 0 else 1.0
        return {
            "domain": self.domain,
            "is_valid": self.is_valid and (failed == 0),
            "pass_count": passed,
            "fail_count": failed,
            "total_rules": total,
            "passed_rules": passed,
            "failed_rules": failed,
            "pass_rate": pass_rate,
            "checks": self.checks
        }



class VerificationEngine:
    """
    Executes dataset and column verifications against a Polars DataFrame.
    """

    @staticmethod
    def verify(df: pl.DataFrame, spec: Dict[str, Any]) -> VerificationReport:
        domain = spec.get("domain", "DATASET")
        report = VerificationReport(domain)
        n_rows = df.height

        if n_rows == 0:
            return report

        # 1. Dataset-level verifications
        dataset_verifs = spec.get("verifications", [])
        if isinstance(dataset_verifs, dict):
            dataset_verifs = [dataset_verifs]

        for verif in dataset_verifs:
            if not isinstance(verif, dict):
                continue

            # Unique key check
            if "unique" in verif:
                cols = verif["unique"].get("columns", [])
                cols = [c for c in cols if c in df.columns]
                if cols:
                    dups = df.select(cols).is_duplicated()
                    dup_count = int(dups.sum())
                    passed = (dup_count == 0)
                    sample_dups = df.filter(dups).select(cols).head(5).to_dicts() if dup_count > 0 else []
                    report.add_result(
                        check_type="DATASET_UNIQUE",
                        target=", ".join(cols),
                        rule_desc=f"Unique composite key: [{', '.join(cols)}]",
                        passed=passed,
                        total_records=n_rows,
                        failing_records=dup_count,
                        sample_failures=sample_dups
                    )

            # Row count check
            if "row_count" in verif:
                rc = verif["row_count"]
                min_c = rc.get("min")
                max_c = rc.get("max")
                passed = True
                if min_c is not None and n_rows < min_c:
                    passed = False
                if max_c is not None and n_rows > max_c:
                    passed = False
                report.add_result(
                    check_type="DATASET_ROW_COUNT",
                    target="ROW_COUNT",
                    rule_desc=f"Row count bounds: min={min_c}, max={max_c}",
                    passed=passed,
                    total_records=n_rows,
                    failing_records=0 if passed else n_rows
                )

            # Predicate check
            if "predicate" in verif:
                when_sql = verif["predicate"].get("when", "")
                if when_sql:
                    pred_expr = SQLParser.parse_to_expr(when_sql, {c: None for c in df.columns})
                    eval_s = df.select(pred_expr.alias("res"))["res"]
                    failing_count = int((~eval_s.fill_null(False)).sum())
                    passed = (failing_count == 0)
                    report.add_result(
                        check_type="DATASET_PREDICATE",
                        target="PREDICATE",
                        rule_desc=f"Predicate: {when_sql}",
                        passed=passed,
                        total_records=n_rows,
                        failing_records=failing_count
                    )

        # 2. Column-level verifications
        columns_spec = spec.get("columns", [])
        if isinstance(columns_spec, list):
            for col_spec in columns_spec:
                col_name = col_spec.get("name")
                if not col_name or col_name not in df.columns:
                    continue

                col_verifs = col_spec.get("verifications", [])
                if isinstance(col_verifs, dict):
                    col_verifs = [col_verifs]

                for cv in col_verifs:
                    if not isinstance(cv, dict):
                        continue

                    # Not missing check
                    if "not_missing" in cv:
                        series = df[col_name]
                        missing_mask = series.is_null() | (series.cast(pl.Utf8, strict=False) == "") | (series.cast(pl.Utf8, strict=False) == "NA")
                        missing_count = int(missing_mask.sum())
                        report.add_result(
                            check_type="COLUMN_NOT_MISSING",
                            target=col_name,
                            rule_desc=f"Column '{col_name}' must not be missing",
                            passed=(missing_count == 0),
                            total_records=n_rows,
                            failing_records=missing_count
                        )

                    # Allowed values check
                    if "allowed_values" in cv:
                        allowed = cv["allowed_values"].get("values", [])
                        str_allowed = [str(v) for v in allowed]
                        series = df[col_name].cast(pl.Utf8, strict=False)
                        non_null_series = series.drop_nulls()
                        if non_null_series.len() > 0:
                            invalid = non_null_series.filter(~non_null_series.is_in(str_allowed))
                            invalid_count = int(invalid.len())
                        else:
                            invalid_count = 0
                        report.add_result(
                            check_type="COLUMN_ALLOWED_VALUES",
                            target=col_name,
                            rule_desc=f"Allowed values: {str_allowed}",
                            passed=(invalid_count == 0),
                            total_records=n_rows,
                            failing_records=invalid_count,
                            sample_failures=invalid.head(5).to_list() if invalid_count > 0 else []
                        )

                    # Range check
                    if "range" in cv:
                        r_min = cv["range"].get("min")
                        r_max = cv["range"].get("max")
                        cond = pl.lit(True)
                        if r_min is not None:
                            cond = cond & (pl.col(col_name).cast(pl.Float64, strict=False) >= r_min)
                        if r_max is not None:
                            cond = cond & (pl.col(col_name).cast(pl.Float64, strict=False) <= r_max)

                        res_s = df.select(cond.alias("range_ok"))["range_ok"]
                        invalid_count = int((~res_s.fill_null(True)).sum())
                        report.add_result(
                            check_type="COLUMN_RANGE",
                            target=col_name,
                            rule_desc=f"Range: min={r_min}, max={r_max}",
                            passed=(invalid_count == 0),
                            total_records=n_rows,
                            failing_records=invalid_count
                        )

                    # Regex match check
                    if "matches" in cv:
                        pattern = cv["matches"].get("pattern", "")
                        str_series = df[col_name].cast(pl.Utf8, strict=False).drop_nulls()
                        matching = str_series.str.contains(pattern)
                        invalid_count = int((~matching).sum())
                        report.add_result(
                            check_type="COLUMN_MATCHES",
                            target=col_name,
                            rule_desc=f"Matches regex pattern: {pattern}",
                            passed=(invalid_count == 0),
                            total_records=n_rows,
                            failing_records=invalid_count
                        )

        # 3. CDISC Baseline Conformance Suite (when no custom rules defined)
        if len(report.checks) == 0:
            # Check 1: Non-empty dataset
            report.add_result(
                check_type="CDISC_NOT_EMPTY",
                rule_id="SD0001",
                target="DATASET",
                rule_desc=f"Dataset contains transformed clinical records (Total: {n_rows} rows)",
                passed=(n_rows > 0),
                total_records=n_rows,
                failing_records=0 if n_rows > 0 else 1
            )
            # Check 2: Unique composite key
            keys = spec.get("keys", [])
            valid_keys = [k for k in keys if k in df.columns]
            if valid_keys:
                dups = df.select(valid_keys).is_duplicated()
                dup_count = int(dups.sum())
                report.add_result(
                    check_type="DATASET_UNIQUE",
                    rule_id="SD0002",
                    target=", ".join(valid_keys),
                    rule_desc=f"Unique composite key integrity: [{', '.join(valid_keys)}]",
                    passed=(dup_count == 0),
                    total_records=n_rows,
                    failing_records=dup_count
                )
            # Check 3: STUDYID presence & non-missing
            if "STUDYID" in df.columns:
                miss_s = int((df["STUDYID"].is_null() | (df["STUDYID"].cast(pl.Utf8, strict=False) == "")).sum())
                report.add_result(
                    check_type="CDISC_STUDYID",
                    rule_id="SD0003",
                    target="STUDYID",
                    rule_desc="Required variable STUDYID is populated across all records",
                    passed=(miss_s == 0),
                    total_records=n_rows,
                    failing_records=miss_s
                )
            # Check 4: DOMAIN match
            if "DOMAIN" in df.columns:
                dom_s = df["DOMAIN"].cast(pl.Utf8, strict=False).drop_nulls()
                dom_ok = (dom_s == domain).all() if dom_s.len() > 0 else False
                report.add_result(
                    check_type="CDISC_DOMAIN",
                    rule_id="SD0004",
                    target="DOMAIN",
                    rule_desc=f"DOMAIN matches domain identifier '{domain}'",
                    passed=bool(dom_ok),
                    total_records=n_rows,
                    failing_records=0 if dom_ok else n_rows
                )
            # Check 5: USUBJID for subject domains
            if domain not in ("TS", "TA", "TE", "TV", "TI") and "USUBJID" in df.columns:
                miss_u = int((df["USUBJID"].is_null() | (df["USUBJID"].cast(pl.Utf8, strict=False) == "")).sum())
                report.add_result(
                    check_type="CDISC_USUBJID",
                    rule_id="SD0005",
                    target="USUBJID",
                    rule_desc="Required variable USUBJID is populated across all subjects",
                    passed=(miss_u == 0),
                    total_records=n_rows,
                    failing_records=miss_u
                )

        return report

