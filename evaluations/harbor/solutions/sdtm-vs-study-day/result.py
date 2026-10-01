# Reference solution for the yamaa benchmark sdtm-vs-study-day (Python track).
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
        # Collection and element dates are ISO dates without times.
        return date.fromisoformat(text[:10])
    except ValueError:
        return None


def study_day(ref: date | None, coll: date | None) -> int | None:
    if ref is None or coll is None:
        return None
    delta = (coll - ref).days
    return delta + 1 if delta >= 0 else delta


dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
se = pl.read_csv("/app/input/se.csv", infer_schema=False)
tv = pl.read_csv("/app/input/tv.csv", infer_schema=False)
vs_raw = pl.read_csv("/app/input/vs_raw.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64)
)

tv_map = {row["VISIT"]: row["VISITNUM"] for row in tv.to_dicts()}
ref_map = {
    row["USUBJID"]: parse_date(row["RFSTDTC"]) for row in dm.to_dicts()
}
se_by_subject: dict[str, list[dict]] = {}
for row in se.to_dicts():
    se_by_subject.setdefault(row["USUBJID"], []).append(
        {
            "EPOCH": row["EPOCH"],
            "start": parse_date(row["SESTDTC"]),
            "end": parse_date(row["SEENDTC"]),
        }
    )

# Latest planned visit number collected before each date, per subject, for
# unplanned labels such as UNSCHEDULED.
planned_by_subject: dict[str, list[tuple[date, float]]] = {}
for row in vs_raw.to_dicts():
    if row["VISIT"] in tv_map and row["VSDTC"] not in (None, ""):
        coll = parse_date(row["VSDTC"])
        if coll is not None:
            planned_by_subject.setdefault(row["USUBJID"], []).append(
                (coll, float(tv_map[row["VISIT"]]))
            )

rows = []
for row in vs_raw.to_dicts():
    usubjid = row["USUBJID"]
    coll = parse_date(row["VSDTC"])
    # Planned visit number, or the latest planned number before the
    # collection date plus .01 for an unplanned label.
    if row["VISIT"] in tv_map:
        visitnum: float | None = float(tv_map[row["VISIT"]])
    else:
        candidates = [
            num for day, num in planned_by_subject.get(usubjid, []) if day < coll
        ] if coll is not None else []
        visitnum = max(candidates) + 0.01 if candidates else None
    # Element containing the collection date; a shared day belongs to the
    # later element.
    epoch = None
    if coll is not None:
        hits = [
            e
            for e in se_by_subject.get(usubjid, [])
            if e["start"] is not None
            and e["end"] is not None
            and e["start"] <= coll <= e["end"]
        ]
        if hits:
            hits.sort(key=lambda e: e["start"])
            epoch = hits[-1]["EPOCH"]
    vsdy = study_day(ref_map.get(usubjid), coll)
    rows.append(
        {
            "DOMAIN": "VS",
            "STUDYID": row["STUDYID"],
            "USUBJID": usubjid,
            "VSSEQ": row["VSSEQ"],
            "VSTESTCD": row["VSTESTCD"],
            "VSORRES": row["VSORRES"],
            "VSDTC": row["VSDTC"],
            "VISIT": row["VISIT"],
            "VISITNUM": visitnum,
            "EPOCH": epoch,
            "VSDY": vsdy,
        }
    )

vs = pl.DataFrame(rows).sort(["USUBJID", "VSSEQ"]).select(
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "VSSEQ",
    "VSTESTCD",
    "VSORRES",
    "VSDTC",
    "VISIT",
    "VISITNUM",
    "EPOCH",
    "VSDY",
)

Path("/app/output").mkdir(exist_ok=True)
vs.write_csv("/app/output/vs.csv")
