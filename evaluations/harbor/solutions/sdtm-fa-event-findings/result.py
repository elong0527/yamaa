# Reference solution for the yamaa benchmark sdtm-fa-event-findings (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM FA dataset (findings about adverse events) from ODM data.

Reads the long-format ODM extract from /app/input and writes one FA
record per answered test (LOC, SIZE, BIOPSY) on each findings form
linked to an adverse event occurrence, to /app/output/fa.csv.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "FASEQ", "FATESTCD", "FATEST",
    "FAOBJ", "FACAT", "FAORRES", "FAORRESU", "FASTRESC", "FASTRESN",
    "FASTRESU", "FALNKID", "FADTC",
]

TEST_ORDER = {"LOC": 0, "SIZE": 1, "BIOPSY": 2}
TEST_NAMES = {"LOC": "Location", "SIZE": "Size", "BIOPSY": "Biopsied"}


def read_odm(input_dir):
    """Return the rows of the ODM long-format extract in /app/input."""
    for name in sorted(os.listdir(input_dir)):
        if not name.lower().endswith(".csv"):
            continue
        path = os.path.join(input_dir, name)
        with open(path, newline="", encoding="utf-8") as handle:
            reader = csv.DictReader(handle)
            fields = reader.fieldnames or []
            if {"StudyOID", "SubjectKey", "ItemOID", "Value"} <= set(fields):
                return list(reader)
    raise FileNotFoundError("No ODM extract found in " + input_dir)


def group_forms(rows):
    """Group ODM rows into form occurrences keyed by their identity.

    Returns (ae_forms, fa_forms): lists of dicts with the occurrence
    keys and an ``items`` mapping of ItemOID to collected value.
    """
    forms = {}
    order = []
    for row in rows:
        key = (
            row["StudyOID"], row["SubjectKey"], row["StudyEventOID"],
            row["StudyEventRepeatKey"], row["ItemGroupOID"],
            row["ItemGroupRepeatKey"],
        )
        if key not in forms:
            forms[key] = {
                "study": row["StudyOID"],
                "subject": row["SubjectKey"],
                "event": row["StudyEventOID"],
                "event_repeat": row["StudyEventRepeatKey"],
                "group": row["ItemGroupOID"],
                "group_repeat": row["ItemGroupRepeatKey"],
                "items": {},
                "position": len(order),
            }
            order.append(key)
        forms[key]["items"][row["ItemOID"]] = row["Value"]
    ae_forms = [forms[k] for k in order if forms[k]["group"] == "IG.AE"]
    fa_forms = [forms[k] for k in order if forms[k]["group"] == "IG.FA"]
    return ae_forms, fa_forms


def as_number(text):
    """Format a collected measurement as a plain number string."""
    value = float(text)
    if value == int(value):
        return str(int(value))
    return str(value)


def main():
    rows = read_odm(INPUT_DIR)
    ae_forms, fa_forms = group_forms(rows)

    # Index AE occurrences by subject and event occurrence so each
    # findings form can read the term of its linked event.
    ae_by_event = {}
    for form in ae_forms:
        key = (form["study"], form["subject"], form["event"],
               form["event_repeat"])
        ae_by_event[key] = form

    records = []
    for form in fa_forms:
        items = form["items"]
        lnkid = items.get("IT.FA.AELNKID", "")
        ae_key = (form["study"], form["subject"], form["event"],
                  form["event_repeat"])
        ae_form = ae_by_event.get(ae_key)
        faobj = ae_form["items"].get("IT.AE.AETERM", "") if ae_form else ""
        event_order = ae_form["position"] if ae_form else form["position"]
        fadtc = items.get("IT.FA.FADTC", "")
        size_unit = items.get("IT.FA.SIZEU", "")

        answers = []
        location = items.get("IT.FA.LOCATION", "")
        if location != "":
            answers.append(("LOC", location, "", "", ""))
        size = items.get("IT.FA.SIZE", "")
        if size != "":
            answers.append(("SIZE", size, size_unit, as_number(size),
                            size_unit))
        biopsy = items.get("IT.FA.BIOPSY", "")
        if biopsy != "":
            answers.append(("BIOPSY", biopsy, "", "", ""))

        for testcd, orres, orresu, stresn, stresu in answers:
            records.append({
                "subject": form["subject"],
                "event_order": event_order,
                "test_order": TEST_ORDER[testcd],
                "tiebreak": orres,
                "row": {
                    "DOMAIN": "FA",
                    "STUDYID": form["study"],
                    "USUBJID": form["subject"],
                    "FASEQ": "",
                    "FATESTCD": testcd,
                    "FATEST": TEST_NAMES[testcd],
                    "FAOBJ": faobj,
                    "FACAT": "AE",
                    "FAORRES": orres,
                    "FAORRESU": orresu,
                    "FASTRESC": orres,
                    "FASTRESN": stresn,
                    "FASTRESU": stresu,
                    "FALNKID": lnkid,
                    "FADTC": fadtc,
                },
            })

    # Subject order follows first appearance in the collected data.
    subject_order = {}
    for row in rows:
        if row["SubjectKey"] not in subject_order:
            subject_order[row["SubjectKey"]] = len(subject_order)

    records.sort(key=lambda r: (subject_order[r["subject"]],
                                r["event_order"], r["test_order"],
                                r["tiebreak"]))
    counters = {}
    for record in records:
        subject = record["subject"]
        counters[subject] = counters.get(subject, 0) + 1
        record["row"]["FASEQ"] = str(counters[subject])

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    out_path = os.path.join(OUTPUT_DIR, "fa.csv")
    with open(out_path, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        for record in records:
            writer.writerow(record["row"])


if __name__ == "__main__":
    main()
