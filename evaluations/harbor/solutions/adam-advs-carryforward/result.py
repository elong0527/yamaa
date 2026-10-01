# Reference solution for the yamaa benchmark adam-advs-carryforward (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

plan = pl.read_csv("/app/input/plan.csv", infer_schema=False).with_columns(
    pl.col("ASEQ").cast(pl.Int64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
)
vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64, strict=False),
    pl.col("VSDTC").str.to_date(strict=False),
    pl.col("VSSTRESN").cast(pl.Float64, strict=False),
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).with_columns(
    pl.col("TRTSDT").str.to_date(strict=False)
)

# The record collected for a planned measurement: same subject, test code
# as the parameter, and collection date as the planned date.
collected = plan.join(
    vs.select("STUDYID", "USUBJID", "VSSEQ", VSTESTCD="VSTESTCD", VSDTC="VSDTC", AVALCOL="VSSTRESN"),
    left_on=["STUDYID", "USUBJID", "PARAMCD", "ADT"],
    right_on=["STUDYID", "USUBJID", "VSTESTCD", "VSDTC"],
    how="left",
)

# The collected value, otherwise the most recent earlier value for the
# same subject and parameter, in planned order.
carried = (
    collected.sort(["STUDYID", "USUBJID", "PARAMCD", "ADT", "ASEQ"])
    .with_columns(
        AVAL=pl.col("AVALCOL")
        .fill_null(strategy="forward")
        .over(["STUDYID", "USUBJID", "PARAMCD"])
    )
    .with_columns(AVAL=pl.coalesce("AVALCOL", "AVAL"))
)

# The latest height on or before treatment start, repeated on every
# record of the subject.
heights = (
    vs.filter(
        (pl.col("VSTESTCD") == "HEIGHT") & pl.col("VSSTRESN").is_not_null()
    )
    .join(adsl.select("STUDYID", "USUBJID", "TRTSDT"), on=["STUDYID", "USUBJID"], how="left")
    .filter(pl.col("VSDTC").is_not_null() & (pl.col("VSDTC") <= pl.col("TRTSDT")))
    .sort(["VSDTC", "VSSEQ"])
    .unique(["STUDYID", "USUBJID"], keep="last", maintain_order=True)
    .select("STUDYID", "USUBJID", HEIGHTBL="VSSTRESN")
)

advs = (
    carried.join(adsl.select("STUDYID", "USUBJID", "TRTSDT"), on=["STUDYID", "USUBJID"], how="left")
    .join(heights, on=["STUDYID", "USUBJID"], how="left")
    .sort(["USUBJID", "ASEQ"])
    .select(
        "STUDYID", "USUBJID", "ASEQ", "VSSEQ", "PARAMCD", "ADT", "AVAL",
        "TRTSDT", "HEIGHTBL",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
