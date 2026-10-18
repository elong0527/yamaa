# Reference solution for the yamaa benchmark sdtm-se-subject-elements (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM SE (Subject Elements) dataset from DM and ODM extracts.

One record per subject per element the subject entered. Dates and study
days follow the epoch-based rules described in the task.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_date(value):
    """Parse an ISO date (or the date part of an ISO datetime).

    Returns a datetime.date, or None when there is no usable value.
    """
    if value is None:
        return None
    text = str(value).strip()
    if not text:
        return None
    try:
        return date.fromisoformat(text[:10])
    except ValueError:
        return None


def study_day(target, reference):
    """SDTM study day of target counted from the reference start date.

    No value when either date has no value. There is no day zero: the
    reference date itself is day 1 and the day before it is day -1.
    """
    if target is None or reference is None:
        return ""
    diff = (target - reference).days
    return diff + 1 if diff >= 0 else diff


def main():
    # DM: one row per subject with the dates the SE rules draw on.
    dm = {}
    with open(f"{INPUT_DIR}/dm.csv", newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            dm[row["USUBJID"]] = {
                "rficdtc": (row.get("RFICDTC") or "").strip(),
                "rfstdtc": parse_date(row.get("RFSTDTC")),
                "rfpendtc": (row.get("RFPENDTC") or "").strip(),
            }

    # ODM extract: one element record per subject per study event, plus
    # the subject's first and last dosing dates (the same on every event).
    elements = {}
    subject_order = []
    with open(f"{INPUT_DIR}/odm.csv", newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            subject = row["SubjectKey"]
            if subject not in elements:
                elements[subject] = {}
                subject_order.append(subject)
            record = elements[subject].setdefault(
                row["StudyEventOID"],
                {
                    "studyid": row["StudyOID"],
                    "usubjid": subject,
                    "etcd": row["StudyEventOID"],
                    "element": "",
                    "taetord": "",
                    "epoch": "",
                    "firstdtc": "",
                    "lastdtc": "",
                },
            )
            item = row["ItemOID"]
            value = (row.get("Value") or "").strip()
            if item == "IT.TE.ELEMENT":
                record["element"] = value
            elif item == "IT.TA.TAETORD":
                record["taetord"] = value
            elif item == "IT.TA.EPOCH":
                record["epoch"] = value
            elif item == "IT.EX.FIRSTDTC":
                record["firstdtc"] = value
            elif item == "IT.EX.LASTDTC":
                record["lastdtc"] = value

    rows = []
    for subject in subject_order:
        subject_dm = dm.get(subject, {})
        reference = subject_dm.get("rfstdtc")
        records = sorted(
            elements[subject].values(),
            key=lambda rec: (int(rec["taetord"]) if rec["taetord"] else 0),
        )
        for record in records:
            epoch = record["epoch"]
            if epoch == "SCREENING":
                sestdtc = subject_dm.get("rficdtc", "")
                seendtc = record["firstdtc"]
            elif epoch == "TREATMENT":
                sestdtc = record["firstdtc"]
                seendtc = record["lastdtc"]
            else:
                sestdtc = record["lastdtc"]
                seendtc = subject_dm.get("rfpendtc", "")
            rows.append(
                {
                    "DOMAIN": "SE",
                    "STUDYID": record["studyid"],
                    "USUBJID": record["usubjid"],
                    "SESEQ": record["taetord"],
                    "ETCD": record["etcd"],
                    "ELEMENT": record["element"],
                    "TAETORD": record["taetord"],
                    "EPOCH": epoch,
                    "SESTDTC": sestdtc,
                    "SEENDTC": seendtc,
                    "SESTDY": study_day(parse_date(sestdtc), reference),
                    "SEENDY": study_day(parse_date(seendtc), reference),
                    "SEUPDES": "",
                }
            )

    columns = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "SESEQ",
        "ETCD",
        "ELEMENT",
        "TAETORD",
        "EPOCH",
        "SESTDTC",
        "SEENDTC",
        "SESTDY",
        "SEENDY",
        "SEUPDES",
    ]
    with open(f"{OUTPUT_DIR}/se.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
