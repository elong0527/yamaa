# Reference solution for the yamaa benchmark adam-advs-windows (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/advs_raw.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64, strict=False),
    pl.col("VISITNUM").cast(pl.Float64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
    pl.col("ADY").cast(pl.Int64, strict=False),
    pl.col("AVAL").cast(pl.Float64, strict=False),
)


def visit(day: int | None) -> tuple[str | None, int | None]:
    if day is None:
        return None, None
    if day < 0:
        return "SCREENING", -1
    if day < 2:
        return "BASELINE", 0
    if day < 22:
        return "WEEK 2", 2
    if day < 43:
        return "WEEK 4", 4
    return "POST-TREATMENT", 99


framed = raw.with_columns(
    pl.col("ADY")
    .map_elements(lambda d: visit(d)[0], return_dtype=pl.String)
    .alias("AVISIT"),
    pl.col("ADY")
    .map_elements(lambda d: visit(d)[1], return_dtype=pl.Int64)
    .alias("AVISITN"),
)

# The earliest record by study day in each study, subject, parameter,
# and visit; the lower sequence number breaks a same-day tie.
flagged = (
    framed.filter(pl.col("AVISIT").is_not_null())
    .sort(["ADY", "VSSEQ"])
    .unique(["STUDYID", "USUBJID", "PARAMCD", "AVISIT"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", "PARAMCD", "AVISIT", "VSSEQ")
    .with_columns(ANL01FL=pl.lit("Y"))
)

advs = (
    framed.join(
        flagged,
        on=["STUDYID", "USUBJID", "PARAMCD", "AVISIT", "VSSEQ"],
        how="left",
    )
    .sort(["STUDYID", "USUBJID", "VSSEQ"])
    .select(
        "STUDYID", "USUBJID", "PARAMCD", "VSSEQ", "VISIT", "VISITNUM",
        "ADT", "ADY", "AVAL", "AVISIT", "AVISITN", "ANL01FL",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
