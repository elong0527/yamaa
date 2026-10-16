# Reference solution for the yamaa benchmark
# adam-adsl-crossover-sequence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
ta = pl.read_csv("/app/input/ta.csv", infer_schema=False).with_columns(
    pl.col("TAETORD").cast(pl.Int64, strict=False)
)
te = pl.read_csv("/app/input/te.csv", infer_schema=False)

# The treatment elements of each arm, numbered in planned order: only the
# elements in an epoch whose name begins with TREATMENT are counted.
ranked = (
    ta.filter(pl.col("EPOCH").str.starts_with("TREATMENT"))
    .sort(["STUDYID", "ARMCD", "TAETORD"])
    .with_columns(
        TRTNUM=pl.col("ETCD").cum_count().over(["STUDYID", "ARMCD"])
    )
    .join(
        te.select("STUDYID", "ETCD", "ELEMENT"),
        on=["STUDYID", "ETCD"],
        how="left",
        maintain_order="left",
    )
)

period1 = ranked.filter(pl.col("TRTNUM") == 1).select(
    "STUDYID", "ARMCD", TRT01P="ELEMENT"
)
period2 = ranked.filter(pl.col("TRTNUM") == 2).select(
    "STUDYID", "ARMCD", TRT02P="ELEMENT"
)

adsl = (
    dm.join(
        period1, on=["STUDYID", "ARMCD"], how="left", maintain_order="left"
    )
    .join(period2, on=["STUDYID", "ARMCD"], how="left", maintain_order="left")
    .with_columns(
        TRTSEQP=pl.when(
            pl.col("TRT01P").is_not_null() & pl.col("TRT02P").is_not_null()
        )
        .then(
            pl.concat_str([pl.col("TRT01P"), pl.col("TRT02P")], separator="/")
        )
        .otherwise(None)
    )
    .select("STUDYID", "USUBJID", "TRT01P", "TRT02P", "TRTSEQP")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
