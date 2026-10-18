# Reference solution for the yamaa benchmark sdtm-suppdm-metadata (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm_raw = pl.read_csv("/app/input/dm_raw.csv", infer_schema=False)

labels = {"RACE1": "Race 1", "RACE2": "Race 2", "RACEOTH": "Race, Other"}
missing = pl.lit(None, dtype=pl.String)
suppdm = pl.concat(
    dm_raw.filter(pl.col(qnam).is_not_null()).select(
        pl.col("STUDYID"),
        pl.lit("DM").alias("RDOMAIN"),
        pl.col("USUBJID"),
        missing.alias("IDVAR"),
        missing.alias("IDVARVAL"),
        pl.lit(qnam).alias("QNAM"),
        pl.lit(label).alias("QLABEL"),
        pl.col(qnam).alias("QVAL"),
        pl.lit("Collected").alias("QORIG"),
        missing.alias("QEVAL"),
    )
    for qnam, label in labels.items()
)

Path("/app/output").mkdir(exist_ok=True)
suppdm.write_csv("/app/output/suppdm.csv")
