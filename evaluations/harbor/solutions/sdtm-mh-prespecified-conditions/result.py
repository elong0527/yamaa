# Reference solution for the yamaa benchmark sdtm-mh-prespecified-conditions (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value) != ""


items = pl.read_csv("/app/input/mh_items.csv", infer_schema=False)
odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

meta = {row["ItemOID"]: (row["MHTERM"], int(row["SORTORD"])) for row in items.to_dicts()}
checklist_oids = {oid for oid in meta if (odm["ItemOID"] == oid).any()}

visit_order = {"SCREENING": 0, "BASELINE": 1}

records_by_subject: dict[tuple[str, str], dict] = {}
for row in odm.to_dicts():
    studyid, usubjid = str(row["StudyOID"]), str(row["SubjectKey"])
    key = (studyid, usubjid)
    records_by_subject.setdefault(key, {"checklist": {}, "volunteered": []})

for row in odm.to_dicts():
    key = (str(row["StudyOID"]), str(row["SubjectKey"]))
    oid, value = str(row["ItemOID"]), row["Value"]
    if oid in checklist_oids:
        # One extract record per asked checklist condition.
        records_by_subject[key]["checklist"][oid] = value if _present(value) else None
    elif not _present(value):
        continue
    else:
        # Each volunteered condition is its own record, even when its
        # repeat number is reused at another visit.
        event = str(row["StudyEventOID"])
        records_by_subject[key]["volunteered"].append(
            {
                "term": str(value),
                "visit_rank": visit_order.get(event, 99),
                "repeat": int(row["ItemGroupRepeatKey"] or 1),
            }
        )

rows = []
for (studyid, usubjid), parts in records_by_subject.items():
    ordered = sorted(parts["checklist"].items(), key=lambda kv: meta[kv[0]][1])
    seq = 0
    for oid, answer in ordered:
        seq += 1
        term, _ = meta[oid]
        answered = answer in ("Y", "N")
        rows.append(
            {
                "DOMAIN": "MH",
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "MHSEQ": str(seq),
                "MHTERM": term,
                "MHCAT": "DISEASE-SPECIFIC HISTORY",
                "MHPRESP": "Y",
                "MHOCCUR": answer if answered else None,
                "MHSTAT": None if answered else "NOT DONE",
            }
        )
    volunteered = sorted(
        parts["volunteered"], key=lambda v: (v["visit_rank"], v["repeat"])
    )
    for entry in volunteered:
        seq += 1
        rows.append(
            {
                "DOMAIN": "MH",
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "MHSEQ": str(seq),
                "MHTERM": entry["term"],
                "MHCAT": "GENERAL HISTORY",
                "MHPRESP": None,
                "MHOCCUR": None,
                "MHSTAT": None,
            }
        )

mh = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "MHSEQ": pl.String,
            "MHTERM": pl.String,
            "MHCAT": pl.String,
            "MHPRESP": pl.String,
            "MHOCCUR": pl.String,
            "MHSTAT": pl.String,
        },
    )
    .with_columns(pl.col("MHSEQ").cast(pl.Int64).alias("_seq"))
    .sort(["USUBJID", "_seq"])
    .select(
        "DOMAIN", "STUDYID", "USUBJID", "MHSEQ", "MHTERM", "MHCAT",
        "MHPRESP", "MHOCCUR", "MHSTAT",
    )
)

Path("/app/output").mkdir(exist_ok=True)
mh.write_csv("/app/output/mh.csv")
