# Reference solution for the yamaa benchmark adam-adae-serious-events (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

# Only serious events are kept; AESER is Y on every row.
adae = ae.filter(pl.col("AESER") == "Y").select(
    "STUDYID", "USUBJID", "AESEQ", "AEDECOD", "AESER"
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
