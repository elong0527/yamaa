# Reference solution for the yamaa benchmark sdtm-qs-questionnaire-items (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value) != ""


items = pl.read_csv("/app/input/items.csv", infer_schema=False)
visits = pl.read_csv("/app/input/visits.csv", infer_schema=False)
notdone = pl.read_csv("/app/input/notdone.csv", infer_schema=False)
odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

item_by_oid = {r["ItemOID"]: r for r in items.to_dicts()}
scores: dict[tuple, str] = {}
for row in odm.to_dicts():
    key = (str(row["SubjectKey"]), str(row["StudyEventOID"]), str(row["ItemOID"]))
    if _present(row["Value"]):
        scores[key] = str(row["Value"]).strip()

refused: dict[tuple, str] = {}
skipped: dict[tuple, str] = {}
for row in notdone.to_dicts():
    key = (str(row["SubjectKey"]), str(row["StudyEventOID"]))
    oid, reason = str(row["ItemOID"] or ""), str(row["Reason"] or "")
    if not _present(oid):
        refused[key] = reason
    else:
        skipped[(key[0], key[1], oid)] = reason

labels = {
    "0": "Not at all",
    "1": "Several days",
    "2": "More than half the days",
    "3": "Nearly every day",
}
total_test = "Patient Health Questionnaire 9 item total score"

visit_by_key = {
    (str(r["SubjectKey"]), str(r["StudyEventOID"])): r for r in visits.to_dicts()
}
first_visit = {}
for row in visits.to_dicts():
    subj = str(row["SubjectKey"])
    num = int(row["VISITNUM"])
    first_visit[subj] = min(first_visit.get(subj, num), num)

rows = []
for (subj, event), visit in visit_by_key.items():
    studyid = str(visit["StudyOID"])
    visitnum = str(visit["VISITNUM"])
    vsdtc = str(visit["VisitDate"])
    blfl = "Y" if int(visitnum) == first_visit[subj] else None
    if (subj, event) in refused:
        rows.append(
            {
                "DOMAIN": "QS",
                "STUDYID": studyid,
                "USUBJID": subj,
                "QSTESTCD": "QSALL",
                "QSTEST": "All Questionnaires",
                "QSCAT": "PHQ-9",
                "QSORRES": None,
                "QSSTRESC": None,
                "QSSTRESN": None,
                "QSSTAT": "NOT DONE",
                "QSREASND": refused[(subj, event)],
                "QSBLFL": blfl,
                "QSDRVFL": None,
                "VISITNUM": visitnum,
                "QSDTC": vsdtc,
            }
        )
        continue
    answered = []
    for oid, item in item_by_oid.items():
        testcd = str(item["QSTESTCD"])
        test = str(item["QSTEST"])
        score = scores.get((subj, event, oid))
        if _present(score):
            answered.append(int(score))
            rows.append(
                {
                    "DOMAIN": "QS",
                    "STUDYID": studyid,
                    "USUBJID": subj,
                    "QSTESTCD": testcd,
                    "QSTEST": test,
                    "QSCAT": "PHQ-9",
                    "QSORRES": labels[score],
                    "QSSTRESC": score,
                    "QSSTRESN": score,
                    "QSSTAT": None,
                    "QSREASND": None,
                    "QSBLFL": blfl,
                    "QSDRVFL": None,
                    "VISITNUM": visitnum,
                    "QSDTC": vsdtc,
                }
            )
        else:
            reason = skipped.get((subj, event, oid))
            rows.append(
                {
                    "DOMAIN": "QS",
                    "STUDYID": studyid,
                    "USUBJID": subj,
                    "QSTESTCD": testcd,
                    "QSTEST": test,
                    "QSCAT": "PHQ-9",
                    "QSORRES": None,
                    "QSSTRESC": None,
                    "QSSTRESN": None,
                    "QSSTAT": "NOT DONE",
                    "QSREASND": reason,
                    "QSBLFL": blfl,
                    "QSDRVFL": None,
                    "VISITNUM": visitnum,
                    "QSDTC": vsdtc,
                }
            )
    # The total appears only for a visit where all nine items were answered.
    if len(answered) == 9:
        total = sum(answered)
        rows.append(
            {
                "DOMAIN": "QS",
                "STUDYID": studyid,
                "USUBJID": subj,
                "QSTESTCD": "PHQ9T",
                "QSTEST": total_test,
                "QSCAT": "PHQ-9",
                "QSORRES": None,
                "QSSTRESC": str(total),
                "QSSTRESN": str(total),
                "QSSTAT": None,
                "QSREASND": None,
                "QSBLFL": blfl,
                "QSDRVFL": "Y",
                "VISITNUM": visitnum,
                "QSDTC": vsdtc,
            }
        )

rows.sort(key=lambda r: (r["USUBJID"], int(r["VISITNUM"]), r["QSTESTCD"]))
for by_subj in sorted({r["USUBJID"] for r in rows}):
    for seq, row in enumerate(
        [r for r in rows if r["USUBJID"] == by_subj], start=1
    ):
        row["QSSEQ"] = str(seq)

qs = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "QSSEQ": pl.String,
            "QSTESTCD": pl.String,
            "QSTEST": pl.String,
            "QSCAT": pl.String,
            "QSORRES": pl.String,
            "QSSTRESC": pl.String,
            "QSSTRESN": pl.String,
            "QSSTAT": pl.String,
            "QSREASND": pl.String,
            "QSBLFL": pl.String,
            "QSDRVFL": pl.String,
            "VISITNUM": pl.String,
            "QSDTC": pl.String,
        },
    )
    .with_columns(
        pl.col("QSSEQ").cast(pl.Int64).alias("_seq"),
        pl.col("VISITNUM").cast(pl.Int64).alias("_visit"),
    )
    .sort(["USUBJID", "_visit", "QSTESTCD"])
    .select(
        "DOMAIN", "STUDYID", "USUBJID", "QSSEQ", "QSTESTCD", "QSTEST", "QSCAT",
        "QSORRES", "QSSTRESC", "QSSTRESN", "QSSTAT", "QSREASND", "QSBLFL",
        "QSDRVFL", "VISITNUM", "QSDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
qs.write_csv("/app/output/qs.csv")
