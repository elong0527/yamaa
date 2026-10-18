# Reference solution for the yamaa benchmark sdtm-mh-prespecified-conditions (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM MH dataset from MH_ITEMS and ODM extracts.

One record per condition asked about on the checklist or volunteered in
free text. Checklist records come first per subject, in form order, then
volunteered conditions by visit (screening before baseline) and form
repeat number.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "MHSEQ",
    "MHTERM",
    "MHCAT",
    "MHPRESP",
    "MHOCCUR",
    "MHSTAT",
]

VISIT_ORDER = {"SCREENING": 0, "BASELINE": 1}


def clean(value):
    """Return the value as a string, treating missing values as blank."""
    if value is None or (isinstance(value, float) and pd.isna(value)):
        return ""
    return str(value)


def main():
    items = pd.read_csv(f"{INPUT_DIR}/mh_items.csv", dtype=str)
    odm = pd.read_csv(f"{INPUT_DIR}/odm.csv", dtype=str)

    term_by_item = dict(zip(items["ItemOID"], items["MHTERM"]))
    order_by_item = {
        item: int(order) for item, order in zip(items["ItemOID"], items["SORTORD"])
    }

    checklist = odm[odm["ItemGroupOID"] == "IG.MH.CHECKLIST"]
    freetext = odm[odm["ItemGroupOID"] == "IG.MH.FREETEXT"]

    records = []
    for subject in sorted(odm["SubjectKey"].unique()):
        subject_records = []

        asked = checklist[checklist["SubjectKey"] == subject].copy()
        asked["_form_order"] = asked["ItemOID"].map(order_by_item)
        asked = asked.sort_values("_form_order", kind="stable")
        for _, row in asked.iterrows():
            answer = clean(row["Value"])
            subject_records.append(
                {
                    "DOMAIN": "MH",
                    "STUDYID": row["StudyOID"],
                    "USUBJID": subject,
                    "MHTERM": term_by_item[row["ItemOID"]],
                    "MHCAT": "DISEASE-SPECIFIC HISTORY",
                    "MHPRESP": "Y",
                    "MHOCCUR": answer,
                    "MHSTAT": "" if answer else "NOT DONE",
                }
            )

        volunteered = freetext[freetext["SubjectKey"] == subject].copy()
        volunteered["_visit_order"] = volunteered["StudyEventOID"].map(
            lambda event: VISIT_ORDER.get(event, len(VISIT_ORDER))
        )
        volunteered["_repeat"] = volunteered["ItemGroupRepeatKey"].astype(int)
        volunteered = volunteered.sort_values(
            ["_visit_order", "_repeat"], kind="stable"
        )
        for _, row in volunteered.iterrows():
            subject_records.append(
                {
                    "DOMAIN": "MH",
                    "STUDYID": row["StudyOID"],
                    "USUBJID": subject,
                    "MHTERM": clean(row["Value"]),
                    "MHCAT": "GENERAL HISTORY",
                    "MHPRESP": "",
                    "MHOCCUR": "",
                    "MHSTAT": "",
                }
            )

        for seq, record in enumerate(subject_records, start=1):
            record["MHSEQ"] = seq
            records.append(record)

    mh = pd.DataFrame(records, columns=COLUMNS)
    mh.to_csv(f"{OUTPUT_DIR}/mh.csv", index=False)


if __name__ == "__main__":
    main()
