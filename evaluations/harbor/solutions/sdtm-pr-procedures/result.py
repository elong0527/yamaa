# Reference solution for the yamaa benchmark sdtm-pr-procedures (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM PR dataset from the ODM extract.

One record per prior-procedure form (prior cancer surgery form and
prior radiotherapy form) collected for a subject.
"""

import csv
import os
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "PRSEQ", "PRTRT", "PRCAT",
    "PRPRESP", "PROCCUR", "PRLOC", "PRLAT", "PRDOSE", "PRDOSU",
    "PRSTDTC", "PRENDTC",
]


def item_name(item_oid):
    """Last component of an item OID, e.g. IT.PR.PRTRT -> PRTRT."""
    return item_oid.rsplit(".", 1)[-1]


def main():
    with open(os.path.join(INPUT_DIR, "odm.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    # Group item values by subject and form instance.
    forms = OrderedDict()
    for row in rows:
        key = (
            row["StudyOID"],
            row["SubjectKey"],
            row["ItemGroupOID"],
            row["ItemGroupRepeatKey"],
        )
        form = forms.setdefault(
            key,
            {
                "study": row["StudyOID"],
                "subject": row["SubjectKey"],
                "group": row["ItemGroupOID"],
                "items": {},
            },
        )
        form["items"][item_name(row["ItemOID"])] = row["Value"]

    records = []
    for form in forms.values():
        items = form["items"]
        if "PRTRT" not in items and "RTPRSP" not in items:
            continue
        rec = {c: "" for c in COLUMNS}
        rec["DOMAIN"] = "PR"
        rec["STUDYID"] = form["study"]
        rec["USUBJID"] = form["study"] + "-" + form["subject"]
        rec["PRTRT"] = items.get("PRTRT", "")
        if "RTPRSP" in items or "RADIOTHERAPY" in form["group"]:
            # Prior radiotherapy form: pre-specified question asked.
            rec["PRCAT"] = "PRIOR RADIOTHERAPY"
            rec["PRPRESP"] = "Y"
            rec["PROCCUR"] = items.get("RTPRSP", "")
            rec["PRDOSE"] = items.get("PRDOSE", "")
            rec["PRDOSU"] = items.get("PRDOSU", "")
        else:
            # Prior cancer surgery form.
            rec["PRCAT"] = "PRIOR CANCER SURGERY"
            rec["PRLOC"] = items.get("PRLOC", "")
            rec["PRLAT"] = items.get("PRLAT", "")
        # Dates keep the precision collected.
        rec["PRSTDTC"] = items.get("PRSTDTC", "")
        rec["PRENDTC"] = items.get("PRENDTC", "")
        records.append(rec)

    # PRSEQ numbers each subject's records from 1 by start date,
    # then by procedure name.
    by_subject = OrderedDict()
    for rec in records:
        by_subject.setdefault(rec["USUBJID"], []).append(rec)
    ordered = []
    for group in by_subject.values():
        group.sort(
            key=lambda r: (r["PRSTDTC"] == "", r["PRSTDTC"], r["PRTRT"])
        )
        for i, rec in enumerate(group, start=1):
            rec["PRSEQ"] = str(i)
        ordered.extend(group)

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "pr.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(ordered)


if __name__ == "__main__":
    main()
