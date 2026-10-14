# Reference solution for the yamaa benchmark adam-adeg-time-matched-concentration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

MATCH = ["STUDYID", "USUBJID", "APERIOD", "APERDAY", "ATPTN"]
NUMBERS = {
    "APERIOD": pl.Int64,
    "APERDAY": pl.Int64,
    "ATPTN": pl.Float64,
    "AVAL": pl.Float64,
}

adeg = pl.read_csv("/app/input/adeg_raw.csv", infer_schema=False).with_columns(
    pl.col(name).cast(kind) for name, kind in {
        **NUMBERS, "BASE": pl.Float64, "CHG": pl.Float64,
    }.items()
)
adpc = pl.read_csv("/app/input/adpc.csv", infer_schema=False).with_columns(
    pl.col(name).cast(kind) for name, kind in NUMBERS.items()
)
columns = adeg.columns + ["ECGPCFL", "PKCONC", "CQTANLFL"]

# The parent drug's sample at exactly the same subject, period, period day
# and nominal time point; metabolite records are never used.
parent = adpc.filter(pl.col("PARAMCD") == "DRGX").select(
    *MATCH, pl.lit(True).alias("SAMPLED"), pl.col("AVAL").alias("PKAVAL")
)

adeg = (
    adeg.join(parent, on=MATCH, how="left", validate="m:1",
              maintain_order="left")
    .with_columns(
        # A sample whatever its result, also below the limit.
        ECGPCFL=pl.when(pl.col("SAMPLED")).then(pl.lit("Y")),
        # As recorded; nothing imputed, except 0 for placebo without a
        # quantified result.
        PKCONC=pl.when(pl.col("PKAVAL").is_not_null())
        .then(pl.col("PKAVAL"))
        .when(pl.col("TRTA") == "Placebo")
        .then(pl.lit(0.0)),
    )
    .with_columns(
        CQTANLFL=pl.when(
            pl.col("CHG").is_not_null() & pl.col("PKCONC").is_not_null()
        ).then(pl.lit("Y")),
    )
    .select(columns)
)

Path("/app/output").mkdir(exist_ok=True)
adeg.write_csv("/app/output/adeg.csv")
