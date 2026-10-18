# Reference solution for the yamaa benchmark sdtm-cm-collected-medications (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM CM dataset from the long-format ODM extract.

One record per medication: a medication is one ItemGroupRepeatKey group
within a subject. Reads /app/input/odm.csv and writes /app/output/cm.csv.
"""

import csv
import re
from collections import OrderedDict

INPUT = "/app/input/odm.csv"
OUTPUT = "/app/output/cm.csv"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "CMSEQ", "CMTRT", "CMINDC", "CMDOSE",
    "CMDOSTXT", "CMDOSU", "CMDOSFRQ", "CMROUTE", "CMSTDTC", "CMENDTC",
    "CMSTRTPT", "CMSTTPT", "CMENRTPT", "CMENTPT",
]

FREQ_MAP = {"Once daily": "QD", "Twice daily": "BID"}
ROUTE_MAP = {"By mouth": "ORAL"}

PLAIN_NUMBER = re.compile(r"^[+-]?(\d+(\.\d*)?|\.\d+)$")


def dose_fields(text):
    """Split collected dose text into (CMDOSE, CMDOSTXT)."""
    if text is None or text == "":
        return "", ""
    if PLAIN_NUMBER.match(text.strip()):
        value = float(text)
        number = str(int(value)) if value == int(value) else repr(value)
        return number, ""
    return "", text


def main():
    with open(INPUT, newline="", encoding="utf-8") as fh:
        rows = list(csv.DictReader(fh))

    # Group item rows into medication records, preserving collected order.
    records = OrderedDict()
    for row in rows:
        key = (row["StudyOID"], row["SubjectKey"], row["ItemGroupRepeatKey"])
        record = records.setdefault(key, {})
        record[row["ItemOID"]] = row["Value"]

    out_rows = []
    for (study_oid, subject_key, repeat), items in records.items():
        cmdose, cmdostxt = dose_fields(items.get("IT.CM.CMDSTXT"))

        prior = items.get("IT.CM.CMPRIOR") == "Y"
        ongoing = items.get("IT.CM.CMONGO") == "Y"

        out_rows.append({
            "DOMAIN": "CM",
            "STUDYID": study_oid,
            "USUBJID": subject_key,
            "CMSEQ": repeat,
            "CMTRT": items.get("IT.CM.CMTRT", ""),
            "CMINDC": items.get("IT.CM.CMINDC", ""),
            "CMDOSE": cmdose,
            "CMDOSTXT": cmdostxt,
            "CMDOSU": items.get("IT.CM.CMDOSU", ""),
            "CMDOSFRQ": FREQ_MAP.get(items.get("IT.CM.CMDOSFRQ", ""),
                                     items.get("IT.CM.CMDOSFRQ", "")),
            "CMROUTE": ROUTE_MAP.get(items.get("IT.CM.CMROUTE", ""),
                                     items.get("IT.CM.CMROUTE", "")),
            "CMSTDTC": items.get("IT.CM.CMSTDTC", ""),
            "CMENDTC": items.get("IT.CM.CMENDTC", ""),
            "CMSTRTPT": "BEFORE" if prior else "",
            "CMSTTPT": "SCREENING" if prior else "",
            "CMENRTPT": "ONGOING" if ongoing else "",
            "CMENTPT": "END OF STUDY" if ongoing else "",
        })

    with open(OUTPUT, "w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
