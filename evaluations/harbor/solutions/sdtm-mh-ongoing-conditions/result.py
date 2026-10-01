# Reference solution for the yamaa benchmark sdtm-mh-ongoing-conditions (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value) != ""


mh_form = pl.read_csv("/app/input/mh_form.csv", infer_schema=False)

rows = []
for row in mh_form.to_dicts():
    year, month = row.get("MHSTYY"), row.get("MHSTMM")
    if not _present(year):
        mhstdtc = None
    elif not _present(month):
        mhstdtc = str(year)
    else:
        mhstdtc = f"{year}-{str(month).zfill(2)}"

    # A condition still active at screening carries no end date.
    ongoing = str(row.get("MHONGO") or "") == "Y"
    endtc = row.get("MHENDTC")
    if ongoing or not _present(endtc):
        mhendtc = None
    else:
        mhendtc = str(endtc)

    scrfdtc = str(row.get("SCRFDTC") or "")
    if ongoing:
        mhenrtpt = "ONGOING"
    elif _present(endtc) and _present(scrfdtc) and str(endtc) < scrfdtc:
        mhenrtpt = "BEFORE"
    else:
        mhenrtpt = None
    mhentpt = "SCREENING" if mhenrtpt is not None else None

    rows.append(
        {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "MHSEQ": row["MHSEQ"],
            "MHTERM": row["MHTERM"],
            "MHSTDTC": mhstdtc,
            "MHENDTC": mhendtc,
            "MHENRTPT": mhenrtpt,
            "MHENTPT": mhentpt,
        }
    )

mh = (
    pl.DataFrame(
        rows,
        schema={
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "MHSEQ": pl.String,
            "MHTERM": pl.String,
            "MHSTDTC": pl.String,
            "MHENDTC": pl.String,
            "MHENRTPT": pl.String,
            "MHENTPT": pl.String,
        },
    )
    .with_columns(pl.col("MHSEQ").cast(pl.Int64).alias("_seq"))
    .sort(["USUBJID", "_seq"])
    .select(
        "STUDYID", "USUBJID", "MHSEQ", "MHTERM", "MHSTDTC", "MHENDTC",
        "MHENRTPT", "MHENTPT",
    )
)

Path("/app/output").mkdir(exist_ok=True)
mh.write_csv("/app/output/mh.csv")
