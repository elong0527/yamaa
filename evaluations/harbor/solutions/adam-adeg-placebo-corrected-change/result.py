# Reference solution for the yamaa benchmark adam-adeg-placebo-corrected-change (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

COLUMNS = [
    "STUDYID", "USUBJID", "COHORT", "TRTA", "PARAMCD", "PARAM", "APERIOD",
    "APERDAY", "ATPT", "ATPTN", "AVAL", "AVALU", "ABLFL", "BASE", "CHG",
    "BASETYPE", "ACOMPFL", "COMP", "COMPBASE", "COMPCHG", "CCOMPCHG",
    "COMPTYPE",
]
GROUP = ["PARAMCD", "ATPTN"]

adeg = pl.read_csv(
    "/app/input/adeg_raw.csv",
    infer_schema=False,
).with_columns(
    pl.col("APERIOD").cast(pl.Int64),
    pl.col("APERDAY").cast(pl.Int64),
    pl.col("ATPTN").cast(pl.Float64),
    pl.col("AVAL").cast(pl.Float64),
    pl.col("BASE").cast(pl.Float64),
    pl.col("CHG").cast(pl.Float64),
)

after_baseline = pl.col("ABLFL").is_null() | (pl.col("ABLFL") != "Y")
adeg = adeg.with_columns(
    ACOMPFL=pl.when(after_baseline & (pl.col("TRTA") == "Placebo"))
    .then(pl.lit("Y")),
)

# The placebo group: every comparator record of the same parameter and
# nominal time point, all cohorts together. Each mean skips missing values.
placebo = (
    adeg.filter(pl.col("ACOMPFL") == "Y")
    .group_by(GROUP)
    .agg(
        COMP=pl.col("AVAL").mean(),
        COMPBASE=pl.col("BASE").mean(),
        COMPCHG=pl.col("CHG").mean(),
    )
)

adeg = (
    adeg.join(placebo, on=GROUP, how="left", maintain_order="left")
    .with_columns(
        # Baseline records carry no comparator; placebo records carry the
        # group's means like every other record after baseline.
        *[
            pl.when(after_baseline).then(pl.col(c)).alias(c)
            for c in ("COMP", "COMPBASE", "COMPCHG")
        ],
        COMPTYPE=pl.when(after_baseline).then(
            pl.concat_str(pl.lit("Placebo group at "), pl.col("ATPT"))
        ),
    )
    .with_columns(
        # The double delta, on drug records only.
        CCOMPCHG=pl.when(after_baseline & pl.col("ACOMPFL").is_null())
        .then(pl.col("CHG") - pl.col("COMPCHG")),
    )
    .select(COLUMNS)
)

Path("/app/output").mkdir(exist_ok=True)
adeg.write_csv("/app/output/adeg.csv")
