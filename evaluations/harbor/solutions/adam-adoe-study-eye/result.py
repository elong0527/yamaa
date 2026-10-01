# Reference solution for the yamaa benchmark adam-adoe-study-eye (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False)
oe = pl.read_csv("/app/input/oe.csv", infer_schema=False).with_columns(
    pl.col("OESEQ").cast(pl.Int64),
    pl.col("OESTRESN").cast(pl.Float64),
)

# The assigned eye belongs to the subject, so the join is by subject; a
# subject without an assigned eye keeps its measurements with no role.
base = oe.join(
    adsl.select("USUBJID", "STUDYEYE"), on="USUBJID", how="left"
)

study_missing = pl.col("STUDYEYE").is_null() | (pl.col("STUDYEYE") == "")
collected_missing = pl.col("OELAT").is_null() | (pl.col("OELAT") == "")
afeye = (
    pl.when(study_missing | collected_missing)
    .then(None)
    .when(pl.col("OELAT") == "BILATERAL")
    .then(pl.lit("Both Eyes"))
    .when(pl.col("STUDYEYE") == "BILATERAL")
    .then(pl.lit("Study Eye"))
    .when(pl.col("OELAT") == pl.col("STUDYEYE"))
    .then(pl.lit("Study Eye"))
    .otherwise(pl.lit("Fellow Eye"))
)

adoe = (
    base.with_columns(PARAMCD=pl.col("OETESTCD"), AVAL=pl.col("OESTRESN"), AFEYE=afeye)
    .select("STUDYID", "USUBJID", "OESEQ", "PARAMCD", "OELAT", "AVAL", "AFEYE")
    .sort(["USUBJID", "OESEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
adoe.write_csv("/app/output/adoe.csv")
