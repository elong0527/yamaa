# Reference solution for the yamaa benchmark sdtm-ae-odm-repeated (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM AE dataset from the ODM long-format extract.

One record per reported event: an event is one item-group occurrence on an
AE form occurrence (subject, visit, visit repeat, form, form repeat).
Occurrences without a reported term contribute no record.
"""

import csv

INPUT_PATH = "/app/input/odm.csv"
OUTPUT_PATH = "/app/output/ae.csv"

ITEM_NAMES = {
    "IT.AE.AETERM": "AETERM",
    "IT.AE.AESTDTC": "AESTDTC",
    "IT.AE.AEENDTC": "AEENDTC",
    "IT.AE.AESEV": "AESEV",
    "IT.AE.AESER": "AESER",
}

VISIT_ORDER = {"SCREENING": 0, "BASELINE": 1}


def repeat_number(value):
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    # Group the AE form items by event occurrence, keeping first-appearance
    # order of visits for any visit not in the known visit order.
    events = {}
    visit_rank_seen = {}
    for row in rows:
        if row["FormOID"] != "FO.AE":
            continue
        visit = row["StudyEventOID"]
        if visit not in visit_rank_seen:
            visit_rank_seen[visit] = len(visit_rank_seen)
        key = (
            row["StudyOID"],
            row["SubjectKey"],
            visit,
            row["StudyEventRepeatKey"],
            row["FormOID"],
            row["FormRepeatKey"],
            row["ItemGroupOID"],
            row["ItemGroupRepeatKey"],
        )
        event = events.setdefault(key, {})
        field = ITEM_NAMES.get(row["ItemOID"])
        if field is not None:
            event[field] = row["Value"]

    def sort_key(item):
        key, _event = item
        (_study, _subject, visit, visit_repeat, _form, form_repeat,
         _group, group_repeat) = key
        visit_rank = VISIT_ORDER.get(visit, 100 + visit_rank_seen[visit])
        return (
            visit_rank,
            repeat_number(visit_repeat),
            repeat_number(form_repeat),
            repeat_number(group_repeat),
        )

    records_by_subject = {}
    for key, event in events.items():
        term = event.get("AETERM", "")
        if term == "":
            continue
        studyid, usubjid = key[0], key[1]
        records_by_subject.setdefault((studyid, usubjid), []).append(
            (key, event))

    output_rows = []
    for (studyid, usubjid), subject_events in records_by_subject.items():
        subject_events.sort(key=sort_key)
        for seq, (_key, event) in enumerate(subject_events, start=1):
            output_rows.append({
                "DOMAIN": "AE",
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "AESEQ": seq,
                "AETERM": event.get("AETERM", ""),
                "AESTDTC": event.get("AESTDTC", ""),
                "AEENDTC": event.get("AEENDTC", ""),
                "AESEV": event.get("AESEV", ""),
                "AESER": event.get("AESER", ""),
            })

    fieldnames = ["DOMAIN", "STUDYID", "USUBJID", "AESEQ", "AETERM",
                  "AESTDTC", "AEENDTC", "AESEV", "AESER"]
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(output_rows)


if __name__ == "__main__":
    main()
