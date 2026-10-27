"""Create the requested ADaMIG 1.2 ADSL extract from ADSL_RAW."""

import csv
from datetime import datetime
from pathlib import Path


INPUT = Path("/app/input/adsl_raw.csv")
OUTPUT = Path("/app/output/adsl.csv")
COLUMNS = ["STUDYID", "USUBJID", "TRT01A", "AGE", "HEIGHTBL", "TRTSDT", "TRTSDTM"]


def main():
    records = []
    subjects = set()
    with INPUT.open(newline="", encoding="utf-8-sig") as source:
        for raw in csv.DictReader(source):
            subject = (raw["STUDYID"], raw["USUBJID"])
            if subject in subjects:
                raise ValueError(
                    f"ADSL_RAW contains multiple records for subject {subject!r}"
                )
            subjects.add(subject)

            record = {column: raw[column] for column in COLUMNS[:5]}
            treatment_start = raw["TRTSDTC"].strip()
            if treatment_start:
                timestamp = datetime.fromisoformat(treatment_start)
                record["TRTSDTM"] = timestamp.strftime("%Y-%m-%dT%H:%M:%S")
                record["TRTSDT"] = timestamp.date().isoformat()
            else:
                record["TRTSDT"] = ""
                record["TRTSDTM"] = ""
            records.append(record)

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with OUTPUT.open("w", newline="", encoding="utf-8") as destination:
        writer = csv.DictWriter(destination, fieldnames=COLUMNS, lineterminator="\n")
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
