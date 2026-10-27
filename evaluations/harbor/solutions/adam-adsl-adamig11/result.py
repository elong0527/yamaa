"""Create the subject-level ADSL dataset from ADSL_RAW."""

import csv
from datetime import datetime
from pathlib import Path


INPUT = Path("/app/input/adsl_raw.csv")
OUTPUT = Path("/app/output/adsl.csv")
COLUMNS = ["STUDYID", "USUBJID", "TRT01A", "AGE", "HEIGHTBL", "TRTSDT", "TRTSDTM"]


def main():
    with INPUT.open(newline="", encoding="utf-8-sig") as source:
        records = list(csv.DictReader(source))

    subjects = {}
    for raw in records:
        key = (raw["STUDYID"], raw["USUBJID"])
        if key not in subjects:
            subjects[key] = {name: raw.get(name, "") for name in COLUMNS[:5]}
            subjects[key]["TRTSDT"] = ""
            subjects[key]["TRTSDTM"] = ""
        collected = raw.get("TRTSDTC", "").strip()
        if collected:
            # Missing seconds in a collected minute-level timestamp become 00.
            exposure = datetime.fromisoformat(collected)
            timestamp = exposure.strftime("%Y-%m-%dT%H:%M:%S")
            existing = subjects[key]["TRTSDTM"]
            if not existing or timestamp < existing:
                subjects[key]["TRTSDTM"] = timestamp
                subjects[key]["TRTSDT"] = exposure.date().isoformat()

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with OUTPUT.open("w", newline="", encoding="utf-8") as target:
        writer = csv.DictWriter(target, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(subjects.values())


if __name__ == "__main__":
    main()
