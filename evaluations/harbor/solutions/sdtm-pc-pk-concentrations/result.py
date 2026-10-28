"""Convert the laboratory transfer to the SDTM PC domain."""

import csv
from datetime import datetime
from decimal import Decimal
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")
COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "PCSEQ",
    "PCREFID",
    "PCTESTCD",
    "PCTEST",
    "PCORRES",
    "PCORRESU",
    "PCSTRESC",
    "PCSTRESN",
    "PCSTRESU",
    "PCSPEC",
    "PCDTC",
    "PCTPT",
    "PCTPTNUM",
]


def nominal_hours(label):
    if label == "PRE":
        return "0"
    if label == "UNSCHED":
        return ""
    if label.endswith("H"):
        return str(Decimal(label[:-1]))
    raise ValueError(f"Unexpected nominal label: {label!r}")


def main():
    with (INPUT / "lab_transfer.csv").open(newline="", encoding="utf-8-sig") as handle:
        source = list(csv.DictReader(handle))
    source.sort(
        key=lambda r: (
            r["Protocol"],
            r["Subject"],
            datetime.fromisoformat(r["Collected"]),
            r["Sample"],
        )
    )
    sequence = {}
    records = []
    for row in source:
        key = (row["Protocol"], row["Subject"])
        sequence[key] = sequence.get(key, 0) + 1
        result = row["Result"]
        if result != "BLQ":
            Decimal(result)  # Validate numeric content without rounding it.
        records.append(
            dict(
                zip(
                    COLUMNS,
                    [
                        "PC",
                        row["Protocol"],
                        row["Subject"],
                        sequence[key],
                        row["Sample"],
                        "ZX201",
                        "ZX-201",
                        result,
                        "ng/mL",
                        result,
                        "" if result == "BLQ" else result,
                        "ng/mL",
                        "PLASMA",
                        row["Collected"],
                        row["Nominal"],
                        nominal_hours(row["Nominal"]),
                    ],
                )
            )
        )
    OUTPUT.mkdir(exist_ok=True)
    with (OUTPUT / "pc.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
