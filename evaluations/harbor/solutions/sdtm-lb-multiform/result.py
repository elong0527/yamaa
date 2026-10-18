# Reference solution for the yamaa benchmark sdtm-lb-multiform (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from the ODM extract in /app/input.

One record per reported result across the serum (LB), skin biopsy (BX),
saliva (SAL) and tape-strip (TS) forms. Writes /app/output/lb.csv.
"""

import csv
from collections import OrderedDict

INPUT_PATH = "/app/input/odm.csv"
OUTPUT_PATH = "/app/output/lb.csv"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "LBSEQ", "VISIT", "VISITNUM",
    "LBTESTCD", "LBTEST", "LBCAT", "LBSPEC", "LBLOC", "LBORRES",
    "LBORRESU", "LBSTRESC", "LBSTRESN", "LBSTRESU", "LBSTAT", "LBDTC",
]

VISITS = {
    "SCRN": ("SCREENING", "1"),
    "BL": ("BASELINE", "2"),
    "D21": ("DAY 21", "3"),
    "UNSCH": ("UNSCHEDULED", None),
}

# Item name -> (LBTESTCD, LBTEST, LBCAT, LBSPEC, LBLOC, unit)
RESULT_ITEMS = {
    "VITD": ("VITD25OH", "25-Hydroxyvitamin D", "CHEMISTRY", "SERUM",
             "", "ng/mL"),
    "IL13_LES": ("IL13", "Interleukin 13 mRNA", "GENE EXPRESSION",
                 "SKIN BIOPSY", "LESIONAL", "CYCLE"),
    "IL13_NONLES": ("IL13", "Interleukin 13 mRNA", "GENE EXPRESSION",
                    "SKIN BIOPSY", "NON-LESIONAL", "CYCLE"),
    "SAL_CAMP": ("CAMPPRO", "Cathelicidin Protein",
                 "ANTIMICROBIAL PEPTIDE", "SALIVA", "", "ng/mL"),
    "TS_CAMP_LES": ("CAMPPRO", "Cathelicidin Protein",
                    "ANTIMICROBIAL PEPTIDE", "TAPE STRIP", "LESIONAL",
                    "ng/mL"),
    "TS_CAMP_NONLES": ("CAMPPRO", "Cathelicidin Protein",
                       "ANTIMICROBIAL PEPTIDE", "TAPE STRIP",
                       "NON-LESIONAL", "ng/mL"),
}

RESULT_GROUPS = {"LB", "BX", "SAL", "TS"}


def last_segment(oid):
    return oid.rsplit(".", 1)[-1]


def format_number(text):
    value = float(text)
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    with open(INPUT_PATH, newline="") as handle:
        rows = list(csv.DictReader(handle))

    # Group items by form instance:
    # (subject, event, event repeat, group, group repeat).
    forms = OrderedDict()
    for row in rows:
        group = last_segment(row["ItemGroupOID"])
        if group not in RESULT_GROUPS:
            continue
        key = (
            row["SubjectKey"],
            last_segment(row["StudyEventOID"]),
            row["StudyEventRepeatKey"],
            group,
            row["ItemGroupRepeatKey"],
        )
        form = forms.setdefault(key, {"study": row["StudyOID"],
                                      "date": "", "results": []})
        item = last_segment(row["ItemOID"])
        if item.endswith("DTC"):
            form["date"] = row["Value"]
        elif item in RESULT_ITEMS:
            form["results"].append((item, row["Value"]))

    # Assign unscheduled-visit occurrence numbers per subject: each
    # distinct (event repeat, group repeat) instance in date order.
    unscheduled = {}
    for (subject, event, event_rep, group, group_rep), form in forms.items():
        if event == "UNSCH" and form["results"]:
            unscheduled.setdefault(subject, {})[
                (event_rep, group_rep)] = form["date"]
    visitnum_unscheduled = {}
    for subject, instances in unscheduled.items():
        ordered = sorted(instances.items(),
                         key=lambda kv: (kv[1], kv[0][0], kv[0][1]))
        for index, (instance_key, _date) in enumerate(ordered, start=1):
            visitnum_unscheduled[(subject, instance_key)] = \
                "2.%02d" % index

    records = []
    for (subject, event, event_rep, group, group_rep), form in forms.items():
        visit, visitnum = VISITS[event]
        if event == "UNSCH":
            visitnum = visitnum_unscheduled.get(
                (subject, (event_rep, group_rep)), "2.01")
        for item, value in form["results"]:
            if value == "":
                continue
            testcd, test, cat, spec, loc, unit = RESULT_ITEMS[item]
            record = {
                "DOMAIN": "LB",
                "STUDYID": form["study"],
                "USUBJID": subject,
                "VISIT": visit,
                "VISITNUM": visitnum,
                "LBTESTCD": testcd,
                "LBTEST": test,
                "LBCAT": cat,
                "LBSPEC": spec,
                "LBLOC": loc,
                "LBDTC": form["date"],
            }
            if value == "NOT DONE":
                record.update({
                    "LBORRES": "", "LBORRESU": "", "LBSTRESC": "",
                    "LBSTRESN": "", "LBSTRESU": "", "LBSTAT": "NOT DONE",
                })
            else:
                record.update({
                    "LBORRES": value, "LBORRESU": unit,
                    "LBSTRESC": value, "LBSTRESN": format_number(value),
                    "LBSTRESU": unit, "LBSTAT": "",
                })
            records.append(record)

    by_subject = {}
    for record in records:
        by_subject.setdefault(record["USUBJID"], []).append(record)

    ordered_records = []
    for subject in sorted(by_subject):
        subject_records = sorted(
            by_subject[subject],
            key=lambda r: (r["LBDTC"], r["LBTESTCD"], r["LBSPEC"],
                           r["LBLOC"]),
        )
        for index, record in enumerate(subject_records, start=1):
            record["LBSEQ"] = str(index)
            ordered_records.append(record)

    with open(OUTPUT_PATH, "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        for record in ordered_records:
            writer.writerow({col: record.get(col, "") for col in COLUMNS})


if __name__ == "__main__":
    main()
