# Reference solution for the yamaa benchmark adam-adsl-dose-adjustment (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

base = pl.read_csv("/app/input/adsl.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)
ec = pl.read_csv("/app/input/ec.csv", infer_schema=False)
fa = pl.read_csv("/app/input/fa.csv", infer_schema=False)

keys = ["STUDYID", "USUBJID"]

def adjusted(sub: pl.DataFrame, column: str) -> pl.DataFrame:
    return (
        sub.filter(pl.col(column).fill_null("").str.strip_chars() != "")
        .select(keys)
        .unique()
        .with_columns(pl.lit(True).alias(f"__adj_{column}"))
    )


def any_records(sub: pl.DataFrame, tag: str) -> pl.DataFrame:
    return (
        sub.select(keys).unique().with_columns(pl.lit(True).alias(f"__any_{tag}"))
    )


ex_adj = adjusted(ex, "EXADJ")
ec_adj = adjusted(ec, "ECADJ")
fa_adj = (
    fa.filter(
        (pl.col("FATESTCD") == "OCCUR")
        & (pl.col("FAOBJ") == "DOSE ADJUSTMENT")
        & (pl.col("FASTRESC") == "Y")
    )
    .select(keys)
    .unique()
    .with_columns(pl.lit(True).alias("__adj_FA"))
)
ex_any = any_records(ex, "EX")
ec_any = any_records(ec, "EC")
fa_any = any_records(fa, "FA")

adsl = (
    base.join(ex_adj, on=keys, how="left", maintain_order="left")
    .join(ec_adj, on=keys, how="left", maintain_order="left")
    .join(fa_adj, on=keys, how="left", maintain_order="left")
    .join(ex_any, on=keys, how="left", maintain_order="left")
    .join(ec_any, on=keys, how="left", maintain_order="left")
    .join(fa_any, on=keys, how="left", maintain_order="left")
    .with_columns(
        DOSADJFL=pl.when(
            (pl.col("__adj_EXADJ") == True)
            | (pl.col("__adj_ECADJ") == True)
            | (pl.col("__adj_FA") == True)
        )
        .then(pl.lit("Y"))
        .when(
            (pl.col("__any_EX") == True)
            | (pl.col("__any_EC") == True)
            | (pl.col("__any_FA") == True)
        )
        .then(pl.lit("N"))
        .otherwise(None)
    )
    .select("STUDYID", "USUBJID", "DOSADJFL")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
