# Reference solution for the yamaa benchmark sdtm-qs-questionnaire-items (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM QS dataset (PHQ-9) from ITEMS, NOTDONE, ODM and VISITS."""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

RESPONSE_TEXT = {
    0: "Not at all",
    1: "Several days",
    2: "More than half the days",
    3: "Nearly every day",
}

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "QSSEQ", "QSTESTCD", "QSTEST",
    "QSCAT", "QSORRES", "QSSTRESC", "QSSTRESN", "QSSTAT", "QSREASND",
    "QSBLFL", "QSDRVFL", "VISITNUM", "QSDTC",
]


def read_csv(name):
    with open(os.path.join(INPUT_DIR, name), newline="", encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def main():
    items = read_csv("items.csv")
    notdone = read_csv("notdone.csv")
    odm = read_csv("odm.csv")
    visits = read_csv("visits.csv")

    # Item metadata keyed by ItemOID, kept in ITEMS order.
    item_by_oid = {row["ItemOID"]: row for row in items}

    # Visit number and collection date per (subject, study event).
    visit_info = {}
    for row in visits:
        visit_info[(row["SubjectKey"], row["StudyEventOID"])] = (
            int(row["VISITNUM"]),
            row["VisitDate"],
        )

    # Collected scores: (subject, event, item oid) -> int score.
    collected = {}
    study_by_subject = {}
    for row in odm:
        study_by_subject[row["SubjectKey"]] = row["StudyOID"]
        if row["ItemOID"] in item_by_oid:
            collected[(row["SubjectKey"], row["StudyEventOID"], row["ItemOID"])] = int(row["Value"])

    # Not-done declarations: whole questionnaire (item oid empty) or per item.
    refused = {}       # (subject, event) -> reason
    skipped = {}       # (subject, event, item oid) -> reason
    for row in notdone:
        key = (row["SubjectKey"], row["StudyEventOID"])
        if row["ItemOID"]:
            skipped[key + (row["ItemOID"],)] = row["Reason"]
        else:
            refused[key] = row["Reason"]

    # Every subject/visit seen in ODM, NOTDONE or VISITS.
    subject_events = set()
    for subject, event in visit_info:
        subject_events.add((subject, event))
    for subject, event, _oid in collected:
        subject_events.add((subject, event))
    for subject, event in refused:
        subject_events.add((subject, event))
    for subject, event, _oid in skipped:
        subject_events.add((subject, event))

    def sort_key(se):
        subject, event = se
        visitnum = visit_info.get((subject, event), (0, ""))[0]
        return (subject, visitnum, event)

    records = []
    for subject, event in sorted(subject_events, key=sort_key):
        visitnum, visit_date = visit_info.get((subject, event), ("", ""))
        studyid = study_by_subject.get(subject, "")
        blfl = "Y" if visitnum == 1 else ""
        base = {
            "DOMAIN": "QS",
            "STUDYID": studyid,
            "USUBJID": subject,
            "QSCAT": "PHQ-9",
            "VISITNUM": visitnum,
            "QSDTC": visit_date,
            "QSBLFL": blfl,
        }
        visit_records = []
        if (subject, event) in refused:
            rec = dict(base)
            rec.update({
                "QSTESTCD": "QSALL",
                "QSTEST": "All Questionnaires",
                "QSORRES": "",
                "QSSTRESC": "",
                "QSSTRESN": "",
                "QSSTAT": "NOT DONE",
                "QSREASND": refused[(subject, event)],
                "QSDRVFL": "",
            })
            visit_records.append(rec)
        else:
            answered = 0
            total = 0
            for item in items:
                oid = item["ItemOID"]
                key = (subject, event, oid)
                rec = dict(base)
                rec.update({
                    "QSTESTCD": item["QSTESTCD"],
                    "QSTEST": item["QSTEST"],
                    "QSDRVFL": "",
                })
                if key in skipped:
                    rec.update({
                        "QSORRES": "",
                        "QSSTRESC": "",
                        "QSSTRESN": "",
                        "QSSTAT": "NOT DONE",
                        "QSREASND": skipped[key],
                    })
                elif key in collected:
                    score = collected[key]
                    answered += 1
                    total += score
                    rec.update({
                        "QSORRES": RESPONSE_TEXT[score],
                        "QSSTRESC": str(score),
                        "QSSTRESN": score,
                        "QSSTAT": "",
                        "QSREASND": "",
                    })
                else:
                    continue
                visit_records.append(rec)
            if answered == len(items):
                rec = dict(base)
                rec.update({
                    "QSTESTCD": "PHQ9T",
                    "QSTEST": "Patient Health Questionnaire 9 item total score",
                    "QSORRES": "",
                    "QSSTRESC": str(total),
                    "QSSTRESN": total,
                    "QSSTAT": "",
                    "QSREASND": "",
                    "QSDRVFL": "Y",
                })
                visit_records.append(rec)
        # QSSEQ within the visit follows QSTESTCD order; the total-score
        # record (PHQ9T) and the refused record (QSALL) sort last anyway.
        visit_records.sort(key=lambda r: r["QSTESTCD"])
        records.extend(visit_records)

    # QSSEQ numbers each subject's records from 1.
    seq_by_subject = {}
    for rec in records:
        subject = rec["USUBJID"]
        seq_by_subject[subject] = seq_by_subject.get(subject, 0) + 1
        rec["QSSEQ"] = seq_by_subject[subject]

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "qs.csv"), "w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=COLUMNS)
        writer.writeheader()
        for rec in records:
            writer.writerow({col: rec.get(col, "") for col in COLUMNS})


if __name__ == "__main__":
    main()
