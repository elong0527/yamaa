# Reference solution for the yamaa benchmark adam-adpp-carryover-exclusion (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

pp = pl.read_csv("/app/input/pp.csv", infer_schema=False).with_columns(
    pl.col("PPSTRESN").cast(pl.Float64, strict=False),
    pl.col("VISITNUM").cast(pl.Int64),
)
pc = pl.read_csv("/app/input/pc.csv", infer_schema=False).with_columns(
    pl.col("PCSTRESN").cast(pl.Float64, strict=False),
)

# The period's pre-dose sample: planned at PREDOSE, with the parameter's
# reference dose time. A below-limit result ("<" and the limit) counts as 0;
# a sample not done has no result and gives no value.
predose = (
    pc.filter(pl.col("PCTPT") == "PREDOSE")
    .with_columns(
        PRECONC=pl.when(pl.col("PCSTRESC").fill_null("").str.starts_with("<"))
        .then(pl.lit(0.0))
        .otherwise(pl.col("PCSTRESN"))
    )
    .select(
        "STUDYID", "USUBJID", pl.col("PCRFTDTC").alias("PPRFTDTC"), "PRECONC"
    )
)
if predose.select(["STUDYID", "USUBJID", "PPRFTDTC"]).is_duplicated().any():
    raise SystemExit("more than one pre-dose sample in a period")

# The same subject's Cmax in the same period.
cmax = pp.filter(pl.col("PPTESTCD") == "CMAX").select(
    "STUDYID", "USUBJID", "PPRFTDTC", pl.col("PPSTRESN").alias("CMAX")
)
if cmax.select(["STUDYID", "USUBJID", "PPRFTDTC"]).is_duplicated().any():
    raise SystemExit("more than one Cmax in a period")

keys = ["STUDYID", "USUBJID", "PPRFTDTC"]
adpp = (
    pp.join(predose, on=keys, how="left", maintain_order="left")
    .join(cmax, on=keys, how="left", maintain_order="left")
    .with_columns(
        PREPCMAX=pl.when(pl.col("CMAX") != 0).then(
            100 * pl.col("PRECONC") / pl.col("CMAX")
        )
    )
    .with_columns(
        # Left out only when the percentage is known and above 5.
        ANL01FL=pl.when(
            pl.col("PREPCMAX").is_not_null() & (pl.col("PREPCMAX") > 5)
        )
        .then(pl.lit(None, dtype=pl.String))
        .otherwise(pl.lit("Y"))
    )
    .select(
        "STUDYID",
        "USUBJID",
        "VISITNUM",
        "VISIT",
        pl.col("PPTESTCD").alias("PARAMCD"),
        pl.col("PPTEST").alias("PARAM"),
        pl.col("PPSTRESN").alias("AVAL"),
        "PRECONC",
        "PREPCMAX",
        "ANL01FL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adpp.write_csv("/app/output/adpp.csv")
