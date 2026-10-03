# Reference solution for the yamaa benchmark adam-advs-windows (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

# SDTM VS staged as the ADaM input; rename to the analysis names the
# rest of the solution works with.
raw = (
    pl.read_csv("/app/input/vs.csv", infer_schema=False)
    .rename(
        {
            "VSTESTCD": "PARAMCD",
            "VSDTC": "ADT",
            "VSDY": "ADY",
            "VSSTRESN": "AVAL",
        }
    )
    .with_columns(
        pl.col("VSSEQ").cast(pl.Int64, strict=False),
        pl.col("VISITNUM").cast(pl.Float64, strict=False),
        pl.col("ADT").str.to_date(strict=False),
        pl.col("ADY").cast(pl.Int64, strict=False),
        pl.col("AVAL").cast(pl.Float64, strict=False),
    )
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

# SV lists every planned visit of every subject, including those that did
# not take place, and the unscheduled visits.
sv = pl.read_csv("/app/input/sv.csv", infer_schema=False).with_columns(
    pl.col("VISITNUM").cast(pl.Float64, strict=False)
)

# Each planned SCREENING, BASELINE, WEEK 2, or WEEK 4 visit gets one SYSBP
# expected record when no collected record's study day falls in the
# analysis window of that name: the planned visit name and number and the
# window, and no date, study day, or value. No other visit gets one.
# Expected records continue the subject's sequence numbering after the
# highest collected VSSEQ, in VISITNUM order.
windows = {"SCREENING": -1, "BASELINE": 0, "WEEK 2": 2, "WEEK 4": 4}
keys = ["STUDYID", "USUBJID", "PARAMCD"]
covered = framed.select(*keys, "AVISIT").drop_nulls().unique()
top = framed.group_by(keys).agg(pl.col("VSSEQ").max().alias("TOP"))

expected = (
    sv.filter(pl.col("VISIT").is_in(list(windows)))
    .with_columns(
        PARAMCD=pl.lit("SYSBP"),
        AVISIT=pl.col("VISIT"),
        AVISITN=pl.col("VISIT").replace_strict(windows, return_dtype=pl.Int64),
    )
    .join(covered, on=[*keys, "AVISIT"], how="anti")
    .join(top, on=keys, how="left")
    .sort([*keys, "VISITNUM"])
    .with_columns(
        VSSEQ=pl.col("TOP").fill_null(0)
        + pl.int_range(1, pl.len() + 1, dtype=pl.Int64).over(keys),
        ADT=pl.lit(None, dtype=pl.Date),
        ADY=pl.lit(None, dtype=pl.Int64),
        AVAL=pl.lit(None, dtype=pl.Float64),
    )
    .select(
        *keys, "VSSEQ", "VISIT", "VISITNUM", "ADT", "ADY", "AVAL",
        "AVISIT", "AVISITN",
    )
)
framed = pl.concat([framed, expected], how="vertical")

# The earliest record by study day in each study, subject, parameter,
# and visit; the lower sequence number breaks a same-day tie. Expected
# records have no study day and never take the flag.
flagged = (
    framed.filter(pl.col("AVISIT").is_not_null() & pl.col("ADY").is_not_null())
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
