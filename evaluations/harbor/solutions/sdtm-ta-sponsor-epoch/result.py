# Reference solution for the yamaa benchmark sdtm-ta-sponsor-epoch (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

design = pl.read_csv("/app/input/design.csv", infer_schema=False)

ta = design.with_columns(
    DOMAIN=pl.lit("TA"),
    TAETORD=pl.col("TAETORD").cast(pl.Int64),
).select(
    "DOMAIN",
    "STUDYID",
    "ARMCD",
    "ARM",
    "TAETORD",
    "ETCD",
    "ELEMENT",
    "EPOCH",
)

Path("/app/output").mkdir(exist_ok=True)
ta.write_csv("/app/output/ta.csv")
