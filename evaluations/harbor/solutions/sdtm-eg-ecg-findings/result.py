# Reference solution for the yamaa benchmark sdtm-eg-ecg-findings (Python
# track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

ODM_CSV = "/app/input/odm.csv"
OUTPUT_CSV = "/app/output/eg.csv"

VISITS = {"SE.DAY1": "DAY 1"}
TIME_POINTS = {
    "FO.EG_PREDOSE": ("PRE-DOSE", 1),
    "FO.EG_2H": ("2 H POST-DOSE", 2),
}
# Item OID: test code, test name, unit (None for a character result).
TESTS = {
    "IT.EG.QT": ("QTAG", "QT Interval, Aggregate", "msec"),
    "IT.EG.INTP": ("INTP", "Interpretation", None),
}
DATETIME_ITEM = "IT.EG.EGDTC"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "EGSEQ", "EGTESTCD", "EGTEST", "EGORRES",
    "EGORRESU", "EGSTRESC", "EGSTRESN", "EGSTRESU", "EGREPNUM", "VISIT",
    "EGDTC", "EGTPT", "EGTPTNUM",
]


def main():
    with open(ODM_CSV, newline="") as f:
        odm = list(csv.DictReader(f))

    # One replicate ECG is one repeat of the ECG item group on a time point's
    # form; collect its items.
    ecgs = {}
    for row in odm:
        if row["FormOID"] not in TIME_POINTS:
            continue
        key = (
            row["StudyOID"],
            row["SubjectKey"],
            row["StudyEventOID"],
            row["FormOID"],
            int(row["ItemGroupRepeatKey"]),
        )
        ecgs.setdefault(key, {})[row["ItemOID"]] = row["Value"].strip()

    records = []
    for (study, subject, event, form, repeat), items in ecgs.items():
        tpt, tptnum = TIME_POINTS[form]
        for item, (testcd, test, unit) in TESTS.items():
            result = items.get(item, "")
            if result == "":
                continue  # a blank result gives no record
            records.append(
                {
                    "DOMAIN": "EG",
                    "STUDYID": study,
                    "USUBJID": subject,
                    "EGTESTCD": testcd,
                    "EGTEST": test,
                    "EGORRES": result,
                    "EGORRESU": unit or "",
                    "EGSTRESC": result,
                    "EGSTRESN": result if unit else "",
                    "EGSTRESU": unit or "",
                    "EGREPNUM": repeat,
                    "VISIT": VISITS[event],
                    "EGDTC": items.get(DATETIME_ITEM, ""),
                    "EGTPT": tpt,
                    "EGTPTNUM": tptnum,
                }
            )

    records.sort(
        key=lambda r: (
            r["STUDYID"], r["USUBJID"], r["EGTPTNUM"], r["EGREPNUM"],
            r["EGTESTCD"],
        )
    )
    sequence = {}
    for record in records:
        subject = (record["STUDYID"], record["USUBJID"])
        sequence[subject] = sequence.get(subject, 0) + 1
        record["EGSEQ"] = sequence[subject]

    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
