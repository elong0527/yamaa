# Reference solution for the yamaa benchmark sdtm-sv-subject-visits (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import date
from pathlib import Path

import polars as pl


def parse_date(text: str | None) -> date | None:
    if text is None or text == "":
        return None
    try:
        return date.fromisoformat(text[:10])
    except ValueError:
        return None


def study_day(ref: date | None, coll: date | None) -> int | None:
    if ref is None or coll is None:
        return None
    delta = (coll - ref).days
    return delta + 1 if delta >= 0 else delta


dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

ref_map = {row["USUBJID"]: parse_date(row["RFSTDTC"]) for row in dm.to_dicts()}
study_map = {row["USUBJID"]: row["STUDYID"] for row in dm.to_dicts()}

date_items = {"IT.VS.VSDTC", "IT.LB.LBDTC", "IT.EX.EXDTC"}

visits: dict[tuple[str, str, str], dict] = {}
for row in odm.to_dicts():
    key = (row["SubjectKey"], row["StudyEventOID"], row["StudyEventRepeatKey"])
    entry = visits.setdefault(
        key,
        {
            "subject": row["SubjectKey"],
            "study": row["StudyOID"],
            "tv": {},
            "dates": [],
        },
    )
    item = row["ItemOID"]
    value = row["Value"]
    if item.startswith("IT.TV."):
        entry["tv"][item] = value
    elif item in date_items and value not in (None, ""):
        entry["dates"].append(value)

records = []
for entry in visits.values():
    tv = entry["tv"]
    dates = sorted(entry["dates"])
    if not dates:
        continue
    start, end = dates[0], dates[-1]
    ref = ref_map.get(entry["subject"])
    records.append(
        {
            "DOMAIN": "SV",
            "STUDYID": entry["study"],
            "USUBJID": entry["subject"],
            "VISIT": tv.get("IT.TV.VISIT"),
            "VISITNUM": float(tv["IT.TV.VISITNUM"]) if tv.get("IT.TV.VISITNUM") not in (None, "") else None,
            "VISITDY": int(tv["IT.TV.VISITDY"]) if tv.get("IT.TV.VISITDY") not in (None, "") else None,
            "SVSTDTC": start,
            "SVENDTC": end,
            "_start": start,
            "SVSTDY": study_day(ref, parse_date(start)),
            "SVENDY": study_day(ref, parse_date(end)),
            "TAETORD": int(tv["IT.TV.TAETORD"]) if tv.get("IT.TV.TAETORD") not in (None, "") else None,
            "EPOCH": tv.get("IT.TV.EPOCH"),
            "SVUPDES": tv.get("IT.TV.UPDES"),
        }
    )

sv = (
    pl.DataFrame(records)
    .sort(["USUBJID", "_start"])
    .with_columns(SVSEQ=pl.int_range(1, pl.len() + 1).over("USUBJID"))
    .sort(["USUBJID", "SVSEQ"])
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "SVSEQ",
        "VISIT",
        "VISITNUM",
        "VISITDY",
        "SVSTDTC",
        "SVENDTC",
        "SVSTDY",
        "SVENDY",
        "TAETORD",
        "EPOCH",
        "SVUPDES",
    )
)

Path("/app/output").mkdir(exist_ok=True)
sv.write_csv("/app/output/sv.csv")
