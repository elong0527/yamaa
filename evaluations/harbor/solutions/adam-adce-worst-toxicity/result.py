# Reference solution for the yamaa benchmark adam-adce-worst-toxicity (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ce = (
    pl.read_csv("/app/input/ce.csv", infer_schema=False)
    .with_columns(
        pl.col("CESEQ").cast(pl.Int64),
        pl.col("ASTDT").str.to_date(strict=False),
    )
)

adce = ce.with_columns(
    ASEV=pl.col("CESEV"),
    ASEVN=pl.when(pl.col("CESEV") == "MILD")
    .then(1)
    .when(pl.col("CESEV") == "MODERATE")
    .then(2)
    .when(pl.col("CESEV") == "SEVERE")
    .then(3)
    .otherwise(None)
    .cast(pl.Int64),
).with_columns(ATOXGRN=pl.col("ASEVN"))

# The graded event with the greatest grade per subject; ties break by
# earliest date, then lowest sequence, so at most one per subject.
best = (
    adce.filter(pl.col("ATOXGRN").is_not_null())
    .sort(["ATOXGRN", "ASTDT", "CESEQ"], descending=[True, False, False])
    .unique("USUBJID", keep="first", maintain_order=True)
    .select("USUBJID", "CESEQ")
    .with_columns(BEST=pl.lit("Y"))
)

adce = (
    adce.join(best, on=["USUBJID", "CESEQ"], how="left")
    .with_columns(AOCCFL=pl.col("BEST"))
    .select(
        "STUDYID",
        "USUBJID",
        "CESEQ",
        "CETERM",
        "ASTDT",
        "ASEV",
        "ASEVN",
        "ATOXGRN",
        "AOCCFL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adce.write_csv("/app/output/adce.csv")
