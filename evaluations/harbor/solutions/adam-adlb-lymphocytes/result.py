# Reference solution for the yamaa benchmark adam-adlb-lymphocytes (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = (
    pl.read_csv("/app/input/lb.csv", infer_schema=False)
    .with_columns(pl.col("LBSTRESN").cast(pl.Float64, strict=False))
    .rename({"LBTESTCD": "PARAMCD", "LBSTRESN": "AVAL", "LBTEST": "PARAM"})
)

# Keep every collected record unchanged, with DTYPE empty.
collected = raw.select("USUBJID", "PARAMCD", "AVAL", "PARAM", "VISIT").with_columns(
    DTYPE=pl.lit(None, dtype=pl.String)
)

# Differential fraction code -> (absolute code, absolute parameter name).
differentials = {
    "LYMLE": ("LYMPH", "Lymphocytes Abs (10^9/L)"),
    "NEUTLE": ("NEUT", "Neutrophils Abs (10^9/L)"),
    "MONOLE": ("MONO", "Monocytes Abs (10^9/L)"),
    "EOSLE": ("EOS", "Eosinophils Abs (10^9/L)"),
    "BASOLE": ("BASO", "Basophils Abs (10^9/L)"),
}

wbc = raw.filter(pl.col("PARAMCD") == "WBC").select("USUBJID", "VISIT", WBCVAL="AVAL")

frames = []
for frac_code, (abs_code, abs_param) in differentials.items():
    frac = (
        raw.filter(pl.col("PARAMCD") == frac_code)
        .select("USUBJID", "VISIT", FRACVAL="AVAL")
    )
    existing = (
        raw.filter(pl.col("PARAMCD") == abs_code)
        .select("USUBJID", "VISIT")
        .with_columns(pl.lit(True).alias("HAS_ABS"))
    )
    calc = (
        wbc.join(frac, on=["USUBJID", "VISIT"], how="inner")
        .join(existing, on=["USUBJID", "VISIT"], how="left")
        .filter(
            pl.col("WBCVAL").is_not_null()
            & pl.col("FRACVAL").is_not_null()
            & pl.col("HAS_ABS").is_null()
        )
        .with_columns(
            PARAMCD=pl.lit(abs_code),
            AVAL=pl.col("WBCVAL") * pl.col("FRACVAL"),
            PARAM=pl.lit(abs_param),
            DTYPE=pl.lit("CALCULATION"),
        )
        .select("USUBJID", "PARAMCD", "AVAL", "PARAM", "VISIT", "DTYPE")
    )
    frames.append(calc)

adlb = pl.concat([collected, *frames], how="diagonal")

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
