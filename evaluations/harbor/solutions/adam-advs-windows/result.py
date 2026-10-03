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

# A planned visit with no collected record whose study day falls in its
# window still appears as an expected record: the planned visit name and
# number and the window, a continued sequence number, and no date, study
# day, or value. The open-ended post-treatment window never gets one.
planned = [
    ("SCREENING", "SCREENING", 1, -1, -(10**9), -1),
    ("BASELINE", "BASELINE", 2, 0, 1, 1),
    ("WEEK 2", "WEEK 2", 3, 2, 2, 21),
    ("WEEK 4", "WEEK 4", 4, 4, 22, 42),
]

expected_rows = []
for (study, subject, param), group in framed.group_by(
    ["STUDYID", "USUBJID", "PARAMCD"], maintain_order=True
):
    seq = group["VSSEQ"].max()
    days = group["ADY"].drop_nulls()
    for avisit, visit, visitnum, avisitn, lo, hi in planned:
        if not ((days >= lo) & (days <= hi)).any():
            seq += 1
            expected_rows.append(
                {
                    "STUDYID": study,
                    "USUBJID": subject,
                    "PARAMCD": param,
                    "VSSEQ": seq,
                    "VISIT": visit,
                    "VISITNUM": float(visitnum),
                    "ADT": None,
                    "ADY": None,
                    "AVAL": None,
                    "AVISIT": avisit,
                    "AVISITN": avisitn,
                }
            )

expected = pl.DataFrame(
    expected_rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "PARAMCD": pl.String,
        "VSSEQ": pl.Int64,
        "VISIT": pl.String,
        "VISITNUM": pl.Float64,
        "ADT": pl.Date,
        "ADY": pl.Int64,
        "AVAL": pl.Float64,
        "AVISIT": pl.String,
        "AVISITN": pl.Int64,
    },
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
