# Reference solution for the yamaa benchmark sdtm-sv-subject-visits (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM SV dataset from the DM and ODM source datasets.

One record per subject per study event occurrence.  Visit start and end
dates are the earliest and latest of the form dates carried by the
visit's VS, LB and EX forms; study days are counted from the subject's
RFSTDTC in DM with no day zero.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "SVSEQ", "VISIT", "VISITNUM",
    "VISITDY", "SVSTDTC", "SVENDTC", "SVSTDY", "SVENDY", "TAETORD",
    "EPOCH", "SVUPDES",
]

# Item groups whose dates bound the visit.
FORM_GROUPS = {"IG.VS", "IG.LB", "IG.EX"}


def parse_date(text):
    return date.fromisoformat(text)


def study_day(day, reference):
    delta = (day - reference).days
    return delta + 1 if delta >= 0 else delta


def main():
    # Reference start dates from DM, keyed by (STUDYID, USUBJID).
    rfstdtc = {}
    with open(f"{INPUT_DIR}/dm.csv", newline="") as handle:
        for row in csv.DictReader(handle):
            rfstdtc[(row["STUDYID"], row["USUBJID"])] = row["RFSTDTC"]

    # Group the ODM items by subject and study event occurrence.
    events = {}
    order = []
    with open(f"{INPUT_DIR}/odm.csv", newline="") as handle:
        for row in csv.DictReader(handle):
            key = (
                row["StudyOID"],
                row["SubjectKey"],
                row["StudyEventOID"],
                row["StudyEventRepeatKey"],
            )
            if key not in events:
                events[key] = {"tv": {}, "dates": []}
                order.append(key)
            event = events[key]
            if row["ItemGroupOID"] == "IG.TV":
                event["tv"][row["ItemOID"]] = row["Value"]
            elif row["ItemGroupOID"] in FORM_GROUPS and row["Value"]:
                event["dates"].append(row["Value"])

    records = []
    for key in order:
        study_oid, subject_key, _, _ = key
        event = events[key]
        tv = event["tv"]
        dates = sorted(event["dates"])
        svstdtc = dates[0] if dates else ""
        svendtc = dates[-1] if dates else ""
        reference = rfstdtc.get((study_oid, subject_key), "")
        svstdy = svendy = ""
        if reference:
            ref_date = parse_date(reference)
            if svstdtc:
                svstdy = study_day(parse_date(svstdtc), ref_date)
            if svendtc:
                svendy = study_day(parse_date(svendtc), ref_date)
        records.append({
            "DOMAIN": "SV",
            "STUDYID": study_oid,
            "USUBJID": subject_key,
            "VISIT": tv.get("IT.TV.VISIT", ""),
            "VISITNUM": tv.get("IT.TV.VISITNUM", ""),
            "VISITDY": tv.get("IT.TV.VISITDY", ""),
            "SVSTDTC": svstdtc,
            "SVENDTC": svendtc,
            "SVSTDY": svstdy,
            "SVENDY": svendy,
            "TAETORD": tv.get("IT.TV.TAETORD", ""),
            "EPOCH": tv.get("IT.TV.EPOCH", ""),
            "SVUPDES": tv.get("IT.TV.UPDES", ""),
            "_sort": svstdtc,
        })

    # SVSEQ numbers each subject's visits from 1 in SVSTDTC order.
    by_subject = {}
    for record in records:
        by_subject.setdefault(record["USUBJID"], []).append(record)
    sequenced = []
    for subject in by_subject:
        visits = sorted(by_subject[subject], key=lambda r: r["_sort"])
        for index, record in enumerate(visits, start=1):
            record["SVSEQ"] = index
            sequenced.append(record)

    with open(f"{OUTPUT_DIR}/sv.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        for record in sequenced:
            writer.writerow({name: record[name] for name in COLUMNS})


if __name__ == "__main__":
    main()
