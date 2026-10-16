# Reference solution for the yamaa benchmark adam-adlb-limit-imputation (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("LBSEQ").cast(pl.Int64),
)

# The numeric boundary of the reported result: the decimal number after an
# optional "<", ">", "<=", ">=" or "=" prefix. It has no value when no
# result was reported.
limit = lb.with_columns(
    LIMITVAL=pl.col("LBSTRESC")
    .str.extract(r"^(?:[<>]=?|=)?([0-9]+(?:\.[0-9]+)?)$", 1)
    .cast(pl.Float64, strict=False)
)

# Only a strict inequality receives the one-unit offset; an inclusive
# limit, an ordinary number and an "=" result keep the number itself.
adlb = limit.with_columns(
    AVAL=pl.when(
        pl.col("LBSTRESC").str.starts_with("<")
        & ~pl.col("LBSTRESC").str.starts_with("<=")
    )
    .then(pl.col("LIMITVAL") - 1)
    .when(
        pl.col("LBSTRESC").str.starts_with(">")
        & ~pl.col("LBSTRESC").str.starts_with(">=")
    )
    .then(pl.col("LIMITVAL") + 1)
    .otherwise(pl.col("LIMITVAL")),
    DTYPE=pl.when(
        pl.col("LBSTRESC").str.starts_with("<")
        | pl.col("LBSTRESC").str.starts_with(">")
    )
    .then(pl.lit("IMPUTATION"))
    .otherwise(None),
).select(
    "STUDYID",
    "USUBJID",
    "LBSEQ",
    pl.col("LBTESTCD").alias("PARAMCD"),
    pl.col("LBSTRESC").alias("AVALC"),
    "AVAL",
    pl.col("LBSTRESU").alias("AVALU"),
    "DTYPE",
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
