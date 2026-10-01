# Reference solution for the yamaa benchmark adam-advs-window-table (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/advs_raw.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
    pl.col("ADY").cast(pl.Int64, strict=False),
    pl.col("AVAL").cast(pl.Float64, strict=False),
)
windows = pl.read_csv("/app/input/awindow.csv", infer_schema=False).with_columns(
    pl.col("AVISITN").cast(pl.Int64, strict=False),
    pl.col("AWLO").cast(pl.Int64, strict=False),
    pl.col("AWHI").cast(pl.Int64, strict=False),
    pl.col("AWTARGET").cast(pl.Int64, strict=False),
)

# The one window in the same study whose first-to-last range, both ends
# inclusive, holds the study day. An absent bound never matches.
assigned = []
for record in raw.sort(["STUDYID", "USUBJID", "VSSEQ"]).iter_rows(named=True):
    match = None
    if record["ADY"] is not None:
        candidates = windows.filter(
            (pl.col("STUDYID") == record["STUDYID"])
            & pl.col("AWLO").is_not_null()
            & pl.col("AWHI").is_not_null()
            & (pl.col("AWLO") <= record["ADY"])
            & (record["ADY"] <= pl.col("AWHI"))
        )
        if candidates.height:
            match = candidates.row(0, named=True)
    assigned.append(
        {
            **record,
            "AVISIT": match["AVISIT"] if match else None,
            "AVISITN": match["AVISITN"] if match else None,
            "AWTARGET": match["AWTARGET"] if match else None,
            "AWTDIFF": record["ADY"] - match["AWTARGET"]
            if (match and record["ADY"] is not None and match["AWTARGET"] is not None)
            else None,
        }
    )

framed = pl.DataFrame(assigned).with_columns(
    pl.col("VSSEQ").cast(pl.Int64),
    pl.col("ADT").cast(pl.Date),
    pl.col("ADY").cast(pl.Int64),
    pl.col("AVAL").cast(pl.Float64),
    pl.col("AVISITN").cast(pl.Int64),
    pl.col("AWTARGET").cast(pl.Int64),
    pl.col("AWTDIFF").cast(pl.Int64),
    AWTABS=pl.col("AWTDIFF").abs(),
)

# The record nearest its target in each study, subject, parameter, and
# visit; the lower sequence number breaks a tie.
flagged = (
    framed.filter(pl.col("AVISIT").is_not_null())
    .sort(["AWTABS", "VSSEQ"])
    .unique(["STUDYID", "USUBJID", "PARAMCD", "AVISIT"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", "PARAMCD", "AVISIT", "VSSEQ")
    .with_columns(ANL01FL=pl.lit("Y"))
)

advs = (
    framed.drop("AWTABS")
    .join(
        flagged,
        on=["STUDYID", "USUBJID", "PARAMCD", "AVISIT", "VSSEQ"],
        how="left",
    )
    .sort(["STUDYID", "USUBJID", "VSSEQ"])
    .select(
        "STUDYID", "USUBJID", "PARAMCD", "VSSEQ", "VISIT", "ADT", "ADY",
        "AVAL", "AVISIT", "AVISITN", "AWTARGET", "AWTDIFF", "ANL01FL",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
