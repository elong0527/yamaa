# Reference solution for the yamaa benchmark sdtm-fa-odm-multitest (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM FA (Findings About) dataset from the ODM extract.

One record per collected reaction finding: for each diary occurrence
(an IG.FA item group repeat within a study event), one record for each
of the occurrence, severity, and longest diameter result items that was
collected. A result item present with a blank result still gives its
record, flagged FASTAT = "NOT DONE".
"""

import csv
import re

INPUT_ODM = "/app/input/odm.csv"
OUTPUT_FA = "/app/output/fa.csv"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "FASEQ", "FATESTCD", "FATEST",
    "FAOBJ", "FACAT", "FAORRES", "FAORRESU", "FASTRESC", "FASTAT",
    "FATPT", "FADTC",
]

# Result items in test order: (item OID suffix, FATESTCD, FATEST).
TESTS = [
    ("OCCUR", "OCCUR", "Occurrence Indicator"),
    ("SEV", "SEV", "Severity/Intensity"),
    ("LDIAM", "LDIAM", "Longest Diameter"),
]


def day_number(study_event_oid):
    """Numeric part of a diary day OID such as DAY1 or DAY2."""
    match = re.search(r"(\d+)\s*$", study_event_oid)
    return int(match.group(1)) if match else 0


def main():
    with open(INPUT_ODM, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    # Group the IG.FA items by diary occurrence: subject, study event
    # (diary day) and its repeat, and the item group (reaction) repeat.
    occurrences = {}
    subject_order = []
    for row in rows:
        if row["ItemGroupOID"] != "IG.FA":
            continue
        key = (
            row["SubjectKey"],
            row["StudyEventOID"],
            row["StudyEventRepeatKey"],
            row["ItemGroupRepeatKey"],
        )
        if key not in occurrences:
            occurrences[key] = {"study": row["StudyOID"], "items": {}}
            if row["SubjectKey"] not in subject_order:
                subject_order.append(row["SubjectKey"])
        item = row["ItemOID"].rsplit(".", 1)[-1]
        occurrences[key]["items"][item] = row["Value"]

    records_by_subject = {}
    for (subject, event_oid, event_repeat, group_repeat), occ in occurrences.items():
        items = occ["items"]
        reaction = items.get("FAOBJ", "")
        fadtc = items.get("FADTC", "")
        day = day_number(event_oid)
        fatpt = "END DAY %d" % day
        for test_index, (item_key, testcd, test) in enumerate(TESTS):
            if item_key not in items:
                # The result item was not collected for this reaction:
                # no record.
                continue
            result = items[item_key]
            record = {
                "DOMAIN": "FA",
                "STUDYID": occ["study"],
                "USUBJID": subject,
                "FATESTCD": testcd,
                "FATEST": test,
                "FAOBJ": reaction,
                "FACAT": "REACTOGENICITY",
                "FAORRES": result,
                "FAORRESU": "mm" if testcd == "LDIAM" else "",
                "FASTRESC": result,
                "FASTAT": "NOT DONE" if result == "" else "",
                "FATPT": fatpt,
                "FADTC": fadtc,
            }
            sort_key = (day, int(event_repeat), int(group_repeat), test_index)
            records_by_subject.setdefault(subject, []).append((sort_key, record))

    out_rows = []
    for subject in subject_order:
        subject_records = sorted(
            records_by_subject.get(subject, []), key=lambda pair: pair[0]
        )
        for seq, (_, record) in enumerate(subject_records, start=1):
            record["FASEQ"] = str(seq)
            out_rows.append(record)

    with open(OUTPUT_FA, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
