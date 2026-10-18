# Reference solution for the yamaa benchmark sdtm-cm-odm-repeated (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT = "/app/input/odm.csv"
OUTPUT = "/app/output/cm.csv"

# Visit ordering: screening before baseline; any other visit follows in
# order of first appearance in the data.
VISIT_ORDER = {"SCREENING": 0, "BASELINE": 1}

with open(INPUT, newline="", encoding="utf-8") as f:
    rows = list(csv.DictReader(f))

visit_seen = {}
for row in rows:
    v = row["StudyEventOID"]
    if v not in visit_seen:
        visit_seen[v] = len(visit_seen)


def visit_key(visit):
    if visit in VISIT_ORDER:
        return (0, VISIT_ORDER[visit])
    return (1, visit_seen[visit])


# Group IG.CM items into courses keyed by subject, visit, visit repeat,
# and form repeat.
courses = OrderedDict()
for row in rows:
    if row["ItemGroupOID"] != "IG.CM":
        continue
    key = (
        row["StudyOID"],
        row["SubjectKey"],
        row["StudyEventOID"],
        int(row["StudyEventRepeatKey"]),
        int(row["ItemGroupRepeatKey"]),
    )
    item = row["ItemOID"].rsplit(".", 1)[-1]
    courses.setdefault(key, {})[item] = row["Value"]

records = []
for (studyid, usubjid, visit, visit_rep, form_rep), items in courses.items():
    cmtrt = items.get("CMTRT", "")
    if not cmtrt:
        # A course with no reported treatment name contributes no record.
        continue
    records.append(
        {
            "STUDYID": studyid,
            "USUBJID": usubjid,
            "CMTRT": cmtrt,
            "CMSTDTC": items.get("CMSTDTC", ""),
            "CMENDTC": items.get("CMENDTC", ""),
            "CMROUTE": items.get("CMROUTE", ""),
            "CMINDC": items.get("CMINDC", ""),
            "_sort": (usubjid, visit_key(visit), visit_rep, form_rep),
        }
    )

records.sort(key=lambda r: r["_sort"])

out = []
seq_by_subject = {}
for rec in records:
    seq_by_subject[rec["USUBJID"]] = seq_by_subject.get(rec["USUBJID"], 0) + 1
    out.append(
        {
            "DOMAIN": "CM",
            "STUDYID": rec["STUDYID"],
            "USUBJID": rec["USUBJID"],
            "CMSEQ": seq_by_subject[rec["USUBJID"]],
            "CMTRT": rec["CMTRT"],
            "CMSTDTC": rec["CMSTDTC"],
            "CMENDTC": rec["CMENDTC"],
            "CMROUTE": rec["CMROUTE"],
            "CMINDC": rec["CMINDC"],
        }
    )

with open(OUTPUT, "w", newline="", encoding="utf-8") as f:
    writer = csv.DictWriter(
        f,
        fieldnames=[
            "DOMAIN",
            "STUDYID",
            "USUBJID",
            "CMSEQ",
            "CMTRT",
            "CMSTDTC",
            "CMENDTC",
            "CMROUTE",
            "CMINDC",
        ],
    )
    writer.writeheader()
    writer.writerows(out)
