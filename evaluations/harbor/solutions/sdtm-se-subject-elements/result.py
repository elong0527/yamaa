# Reference solution for the yamaa benchmark sdtm-se-subject-elements (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import date
from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value) != ""


def _parse(value) -> date | None:
    if not _present(value):
        return None
    return date.fromisoformat(str(value))


def _study_day(ref: date | None, day: date | None) -> str | None:
    if ref is None or day is None:
        return None
    delta = (day - ref).days
    return str(delta + 1 if delta >= 0 else delta)


dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

# Planned element per study event, and dosing dates per subject.
elements: dict[tuple, dict] = {}
dosing: dict[str, dict] = {}
for row in odm.to_dicts():
    subj = str(row["SubjectKey"])
    event = str(row["StudyEventOID"])
    oid, value = str(row["ItemOID"]), row["Value"]
    if oid == "IT.TE.ELEMENT":
        elements.setdefault((subj, event), {})["element"] = str(value)
    elif oid == "IT.TA.TAETORD":
        elements.setdefault((subj, event), {})["order"] = str(value)
    elif oid == "IT.TA.EPOCH":
        elements.setdefault((subj, event), {})["epoch"] = str(value)
    elif oid == "IT.EX.FIRSTDTC":
        dosing.setdefault(subj, {})["first"] = str(value) if _present(value) else None
    elif oid == "IT.EX.LASTDTC":
        dosing.setdefault(subj, {})["last"] = str(value) if _present(value) else None

dm_by_subj = {str(r["USUBJID"]): r for r in dm.to_dicts()}

rows = []
for (subj, etcd), info in elements.items():
    subject = dm_by_subj.get(subj)
    if subject is None:
        continue
    studyid = str(subject["STUDYID"])
    usubjid = str(subject["USUBJID"])
    epoch = info.get("epoch")
    order = info.get("order")
    first = (dosing.get(subj) or {}).get("first")
    last = (dosing.get(subj) or {}).get("last")
    rfic = str(subject.get("RFICDTC") or "")
    rfic = rfic if _present(rfic) else None
    rfstd = str(subject.get("RFSTDTC") or "")
    rfstd = rfstd if _present(rfstd) else None
    rfpen = str(subject.get("RFPENDTC") or "")
    rfpen = rfpen if _present(rfpen) else None
    if epoch == "SCREENING":
        stdtc, endtc = rfic, first
    elif epoch == "TREATMENT":
        stdtc, endtc = first, last
    else:
        # Follow-up, and any other or missing epoch.
        stdtc, endtc = last, rfpen
    ref = _parse(rfstd)
    rows.append(
        {
            "DOMAIN": "SE",
            "STUDYID": studyid,
            "USUBJID": usubjid,
            "SESEQ": order,
            "ETCD": etcd,
            "ELEMENT": info.get("element"),
            "TAETORD": order,
            "EPOCH": epoch,
            "SESTDTC": stdtc,
            "SEENDTC": endtc,
            "SESTDY": _study_day(ref, _parse(stdtc)),
            "SEENDY": _study_day(ref, _parse(endtc)),
            "SEUPDES": None,
        }
    )

rows.sort(key=lambda r: (r["USUBJID"], int(r["TAETORD"] or 0)))

se = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "SESEQ": pl.String,
            "ETCD": pl.String,
            "ELEMENT": pl.String,
            "TAETORD": pl.String,
            "EPOCH": pl.String,
            "SESTDTC": pl.String,
            "SEENDTC": pl.String,
            "SESTDY": pl.String,
            "SEENDY": pl.String,
            "SEUPDES": pl.String,
        },
    )
    .sort(["USUBJID", "TAETORD"])
    .select(
        "DOMAIN", "STUDYID", "USUBJID", "SESEQ", "ETCD", "ELEMENT", "TAETORD",
        "EPOCH", "SESTDTC", "SEENDTC", "SESTDY", "SEENDY", "SEUPDES",
    )
)

Path("/app/output").mkdir(exist_ok=True)
se.write_csv("/app/output/se.csv")
