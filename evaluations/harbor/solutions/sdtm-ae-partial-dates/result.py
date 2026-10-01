# Reference solution for the yamaa benchmark sdtm-ae-partial-dates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import date
from pathlib import Path

import polars as pl


def compose(yr: str | None, mo: str | None, dy: str | None) -> str | None:
    if yr is None or yr == "":
        return None
    if mo is None or mo == "":
        return yr
    mo = mo.zfill(2)
    if dy is None or dy == "":
        return f"{yr}-{mo}"
    return f"{yr}-{mo}-{dy.zfill(2)}"


def complete(yr: str | None, mo: str | None, dy: str | None) -> bool:
    return bool(yr) and bool(mo) and bool(dy)


def study_day(adt: date | None, ref: date | None) -> int | None:
    if adt is None or ref is None:
        return None
    delta = (adt - ref).days
    return delta + 1 if delta >= 0 else delta


odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)
dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)

# One record per form repeat; the repeat number is the sequence number.
# Dates stay at the precision collected: nothing is imputed.
wide = (
    odm.filter(pl.col("ItemGroupOID") == "IG.AE")
    .pivot(
        index=["StudyOID", "SubjectKey", "FormRepeatKey"],
        on="ItemOID",
        values="Value",
        aggregate_function="first",
    )
    .with_columns(AESEQ=pl.col("FormRepeatKey").cast(pl.Int64))
)

ref = {
    row["USUBJID"]: (row["RFSTDTC"] or None) for row in dm.to_dicts()
}

rows = []
for record in wide.to_dicts():
    usubjid = record["SubjectKey"]
    stdtc = compose(
        record.get("IT.AE.AESTYR"), record.get("IT.AE.AESTMO"), record.get("IT.AE.AESTDY")
    )
    endtc = compose(
        record.get("IT.AE.AEENYR"), record.get("IT.AE.AEENMO"), record.get("IT.AE.AEENDY")
    )
    # Only a complete date gets a study day, and only when the reference
    # start date is known. Day 1 is the reference date; there is no day 0.
    rf_raw = ref.get(usubjid)
    try:
        rf = date.fromisoformat(rf_raw) if rf_raw else None
    except ValueError:
        rf = None
    if complete(
        record.get("IT.AE.AESTYR"), record.get("IT.AE.AESTMO"), record.get("IT.AE.AESTDY")
    ):
        try:
            adt = date.fromisoformat(stdtc) if stdtc else None
        except ValueError:
            adt = None
        stdy = study_day(adt, rf)
    else:
        stdy = None
    if complete(
        record.get("IT.AE.AEENYR"), record.get("IT.AE.AEENMO"), record.get("IT.AE.AEENDY")
    ):
        try:
            adt = date.fromisoformat(endtc) if endtc else None
        except ValueError:
            adt = None
        endy = study_day(adt, rf)
    else:
        endy = None
    rows.append(
        {
            "DOMAIN": "AE",
            "STUDYID": record["StudyOID"],
            "USUBJID": usubjid,
            "AESEQ": record["AESEQ"],
            "AETERM": record.get("IT.AE.AETERM"),
            "AESTDTC": stdtc,
            "AEENDTC": endtc,
            "AESTDY": stdy,
            "AEENDY": endy,
        }
    )

ae = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "AESEQ": pl.Int64,
        "AETERM": pl.String,
        "AESTDTC": pl.String,
        "AEENDTC": pl.String,
        "AESTDY": pl.Int64,
        "AEENDY": pl.Int64,
    },
).sort(["USUBJID", "AESEQ"])

Path("/app/output").mkdir(exist_ok=True)
ae.write_csv("/app/output/ae.csv")
