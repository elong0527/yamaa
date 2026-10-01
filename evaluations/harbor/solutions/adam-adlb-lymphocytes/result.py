# Reference solution for the yamaa benchmark adam-adlb-lymphocytes (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/adlb.csv", infer_schema=False).with_columns(
    pl.col("AVAL").cast(pl.Float64, strict=False)
)

# Keep every collected record unchanged, with DTYPE empty.
collected = raw.select("USUBJID", "PARAMCD", "AVAL", "PARAM", "VISIT").with_columns(
    DTYPE=pl.lit(None, dtype=pl.String)
)

# One LYMPH record per subject and visit with both WBC and LYMLE and no
# LYMPH yet: WBC times LYMLE.
wbc = (
    raw.filter(pl.col("PARAMCD") == "WBC")
    .select("USUBJID", "VISIT", WBCVAL="AVAL")
)
lymle = (
    raw.filter(pl.col("PARAMCD") == "LYMLE")
    .select("USUBJID", "VISIT", LYMLEVAL="AVAL")
)
existing = (
    raw.filter(pl.col("PARAMCD") == "LYMPH")
    .select("USUBJID", "VISIT")
    .with_columns(pl.lit(True).alias("HAS_LYMPH"))
)

calc = (
    wbc.join(lymle, on=["USUBJID", "VISIT"], how="inner")
    .join(existing, on=["USUBJID", "VISIT"], how="left")
    .filter(
        pl.col("WBCVAL").is_not_null()
        & pl.col("LYMLEVAL").is_not_null()
        & pl.col("HAS_LYMPH").is_null()
    )
    .with_columns(
        PARAMCD=pl.lit("LYMPH"),
        AVAL=pl.col("WBCVAL") * pl.col("LYMLEVAL"),
        PARAM=pl.lit("Lymphocytes Abs (10^9/L)"),
        DTYPE=pl.lit("CALCULATION"),
    )
    .select("USUBJID", "PARAMCD", "AVAL", "PARAM", "VISIT", "DTYPE")
)

adlb = pl.concat([collected, calc], how="diagonal")

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
