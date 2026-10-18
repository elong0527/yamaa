# Reference solution for the yamaa benchmark sdtm-qs-questionnaire-items (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM QS dataset from ITEMS, NOTDONE, ODM and VISITS.

One record per questionnaire item per subject per visit, plus a
total-score record for every visit where all nine items were answered,
a NOT DONE record for a skipped item, and a single QSALL record for a
refused questionnaire.
"""

from collections import defaultdict

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

RESPONSE_TEXT = {
    "0": "Not at all",
    "1": "Several days",
    "2": "More than half the days",
    "3": "Nearly every day",
}

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "QSSEQ", "QSTESTCD", "QSTEST",
    "QSCAT", "QSORRES", "QSSTRESC", "QSSTRESN", "QSSTAT", "QSREASND",
    "QSBLFL", "QSDRVFL", "VISITNUM", "QSDTC",
]


def read(name):
    return pd.read_csv(
        f"{INPUT_DIR}/{name}", dtype=str, keep_default_na=False
    )


def main():
    items = read("items.csv")
    odm = read("odm.csv")
    notdone = read("notdone.csv")
    visits = read("visits.csv")

    item_by_oid = {
        row["ItemOID"]: row for _, row in items.iterrows()
    }
    item_oids = list(items["ItemOID"])

    visit_info = {}
    for _, row in visits.iterrows():
        visit_info[(row["SubjectKey"], row["StudyEventOID"])] = (
            int(row["VISITNUM"]),
            row["VisitDate"],
        )

    # NOTDONE rows: one naming an item is that item's own record; one
    # naming no item stands for the whole questionnaire.
    skipped = {}
    refused = {}
    study_by_key = {}
    for _, row in notdone.iterrows():
        key = (row["SubjectKey"], row["StudyEventOID"])
        study_by_key[key] = row["StudyOID"]
        if row["ItemOID"]:
            skipped[key + (row["ItemOID"],)] = row["Reason"]
        else:
            refused[key] = row["Reason"]

    # Collected scores keyed by subject, visit and item.
    responses = defaultdict(dict)
    subject_visits = set()
    for _, row in odm.iterrows():
        key = (row["SubjectKey"], row["StudyEventOID"])
        responses[key][row["ItemOID"]] = row["Value"]
        subject_visits.add(key)
        study_by_key[key] = row["StudyOID"]
    subject_visits |= set(refused)
    subject_visits |= set(skipped_key[:2] for skipped_key in skipped)

    records = []
    for subject, visit in subject_visits:
        visitnum, visit_date = visit_info[(subject, visit)]
        blfl = "Y" if visitnum == 1 else ""
        base = {
            "DOMAIN": "QS",
            "STUDYID": study_by_key[(subject, visit)],
            "USUBJID": subject,
            "QSCAT": "PHQ-9",
            "VISITNUM": visitnum,
            "QSDTC": visit_date,
        }
        visit_records = []
        if (subject, visit) in refused:
            visit_records.append({
                **base,
                "QSTESTCD": "QSALL",
                "QSTEST": "All Questionnaires",
                "QSORRES": "",
                "QSSTRESC": "",
                "QSSTRESN": "",
                "QSSTAT": "NOT DONE",
                "QSREASND": refused[(subject, visit)],
                "QSBLFL": blfl,
                "QSDRVFL": "",
                "_rank": 2,
            })
        else:
            answered = responses.get((subject, visit), {})
            all_answered = True
            total = 0
            for oid in item_oids:
                item = item_by_oid[oid]
                record = {
                    **base,
                    "QSTESTCD": item["QSTESTCD"],
                    "QSTEST": item["QSTEST"],
                    "QSBLFL": blfl,
                    "QSDRVFL": "",
                    "_rank": 0,
                }
                skip_key = (subject, visit, oid)
                if skip_key in skipped:
                    all_answered = False
                    record.update({
                        "QSORRES": "",
                        "QSSTRESC": "",
                        "QSSTRESN": "",
                        "QSSTAT": "NOT DONE",
                        "QSREASND": skipped[skip_key],
                    })
                elif oid in answered:
                    score = answered[oid]
                    total += int(score)
                    record.update({
                        "QSORRES": RESPONSE_TEXT[score],
                        "QSSTRESC": score,
                        "QSSTRESN": int(score),
                        "QSSTAT": "",
                        "QSREASND": "",
                    })
                else:
                    all_answered = False
                    continue
                visit_records.append(record)
            if all_answered:
                visit_records.append({
                    **base,
                    "QSTESTCD": "PHQ9T",
                    "QSTEST": "Patient Health Questionnaire 9 item "
                              "total score",
                    "QSORRES": "",
                    "QSSTRESC": str(total),
                    "QSSTRESN": total,
                    "QSSTAT": "",
                    "QSREASND": "",
                    "QSBLFL": blfl,
                    "QSDRVFL": "Y",
                    "_rank": 1,
                })
        # QSSEQ order: visit number, then item records by QSTESTCD,
        # then the total-score record, then the refused record.
        visit_records.sort(key=lambda r: (r["_rank"], r["QSTESTCD"]))
        for record in visit_records:
            record["_visitnum"] = visitnum
        records.extend(visit_records)

    records.sort(key=lambda r: (r["USUBJID"], r["_visitnum"],
                                r["_rank"], r["QSTESTCD"]))
    seq = defaultdict(int)
    for record in records:
        seq[record["USUBJID"]] += 1
        record["QSSEQ"] = seq[record["USUBJID"]]
        del record["_rank"]
        del record["_visitnum"]

    qs = pd.DataFrame(records, columns=COLUMNS)
    qs.to_csv(f"{OUTPUT_DIR}/qs.csv", index=False)


if __name__ == "__main__":
    main()
