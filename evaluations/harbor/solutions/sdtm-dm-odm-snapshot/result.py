# Reference solution for the yamaa benchmark sdtm-dm-odm-snapshot (Python
# track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DM dataset from the EDC's ODM 1.3 XML snapshot.

One record per subject, with columns:
DOMAIN, STUDYID, USUBJID, SUBJID, BRTHDTC, AGE, AGEU, SEX
"""

import csv
import xml.etree.ElementTree as ET

INPUT_PATH = "/app/input/odm.xml"
OUTPUT_PATH = "/app/output/dm.csv"

ODM = "{http://www.cdisc.org/ns/odm/v1.3}"
COLUMNS = ["DOMAIN", "STUDYID", "USUBJID", "SUBJID", "BRTHDTC", "AGE", "AGEU", "SEX"]


def current_values(subject):
    """Each item's current value by its OID. An item recorded as null has no
    value; audit records, signatures and comments hold no items."""
    values = {}
    for item in subject.iter(f"{ODM}ItemData"):
        if item.get("IsNull", "").lower() in ("yes", "true", "1"):
            values[item.get("ItemOID")] = ""
        else:
            values[item.get("ItemOID")] = item.get("Value", "")
    return values


def main():
    root = ET.parse(INPUT_PATH).getroot()
    rows = []
    for clinical in root.iter(f"{ODM}ClinicalData"):
        study = clinical.get("StudyOID")
        for subject in clinical.iter(f"{ODM}SubjectData"):
            key = subject.get("SubjectKey")
            values = current_values(subject)
            age = values.get("IT.DM.AGE", "")
            rows.append(
                {
                    "DOMAIN": "DM",
                    "STUDYID": study,
                    "USUBJID": f"{study}-{key}",
                    "SUBJID": key,
                    "BRTHDTC": values.get("IT.DM.BRTHDAT", ""),
                    "AGE": age,
                    "AGEU": "YEARS" if age else "",
                    "SEX": values.get("IT.DM.SEX", ""),
                }
            )
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
