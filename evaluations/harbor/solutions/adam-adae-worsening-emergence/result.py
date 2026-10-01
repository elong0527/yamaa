# Reference solution for the yamaa benchmark adam-adae-worsening-emergence (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def normalize(column: pl.Expr) -> pl.Expr:
    # Datetimes compare as text once every value carries seconds.
    return (
        pl.when(column.str.len_chars() == 16)
        .then(column + ":00")
        .otherwise(column)
    )


ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).select(
    "USUBJID", "TRTSDTM"
)

base = (
    ae.join(adsl, on="USUBJID", how="left")
    .with_columns(
        ASTDTM=normalize(pl.col("AESTDTC")),
        TRTSDTM=normalize(pl.col("TRTSDTM")),
    )
    .with_columns(
        SEVN=pl.when(pl.col("AESEV") == "MILD")
        .then(1)
        .when(pl.col("AESEV") == "MODERATE")
        .then(2)
        .when(pl.col("AESEV") == "SEVERE")
        .then(3)
        .otherwise(None)
        .cast(pl.Int64),
        TOXN=pl.col("AETOXGR").cast(pl.Int64, strict=False),
    )
)

# The worst severity and grade the same term reaches after exposure.
after = base.filter(
    pl.col("ASTDTM").is_not_null(),
    pl.col("TRTSDTM").is_not_null(),
    pl.col("ASTDTM") >= pl.col("TRTSDTM"),
).group_by(["USUBJID", "AEDECOD"]).agg(
    MAXSEV=pl.col("SEVN").max(), MAXTOX=pl.col("TOXN").max()
)
joined = base.join(after, on=["USUBJID", "AEDECOD"], how="left")

sev_worse = (
    pl.col("MAXSEV").is_not_null()
    & pl.col("SEVN").is_not_null()
    & (pl.col("MAXSEV") > pl.col("SEVN"))
)
tox_worse = (
    pl.col("MAXTOX").is_not_null()
    & pl.col("TOXN").is_not_null()
    & (pl.col("MAXTOX") > pl.col("TOXN"))
)

adae = (
    joined.with_columns(
        TRTEMFL=pl.when(
            pl.col("ASTDTM").is_null() | pl.col("TRTSDTM").is_null()
        )
        .then(None)
        .when(pl.col("ASTDTM") >= pl.col("TRTSDTM"))
        .then(pl.lit("Y"))
        .when(sev_worse | tox_worse)
        .then(pl.lit("Y"))
        .otherwise(None)
    ).select(
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AEDECOD",
        "ASTDTM",
        "TRTSDTM",
        "AESEV",
        "AETOXGR",
        "TRTEMFL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
