"""Derive cycle-level ADVS dosing weights and body surface area."""

import csv
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")
COLUMNS = ["STUDYID", "USUBJID", "ASEQ", "AVISITN", "ADT", "AVAL", "ANCHORWT", "BSA"]


def numeric(value):
    text = (value or "").strip()
    if not text:
        return None
    number = Decimal(text)
    return number if number.is_finite() else None


def formatted(number):
    return "" if number is None else format(number, "f")


def main():
    with (INPUT_DIR / "adsl.csv").open(newline="", encoding="utf-8-sig") as source:
        heights = {
            (row["STUDYID"], row["USUBJID"]): numeric(row["HEIGHTBL"])
            for row in csv.DictReader(source)
        }

    with (INPUT_DIR / "vs.csv").open(newline="", encoding="utf-8-sig") as source:
        visits = list(csv.DictReader(source))

    visits.sort(
        key=lambda row: (
            row["STUDYID"],
            row["USUBJID"],
            Decimal(row["VISITNUM"]),
            Decimal(row["VSSEQ"]),
        )
    )

    anchors = {}
    result = []
    for row in visits:
        subject = (row["STUDYID"], row["USUBJID"])
        weight = numeric(row["VSSTRESN"])
        anchor = anchors.get(subject)
        if weight is not None:
            if (
                anchor is None
                or weight >= anchor * Decimal("1.10")
                or weight <= anchor * Decimal("0.90")
            ):
                anchor = weight
        anchors[subject] = anchor

        height = heights.get(subject)
        bsa = None
        if anchor is not None and height is not None:
            bsa = (
                (anchor * height / Decimal("3600"))
                .sqrt()
                .quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)
            )

        result.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "ASEQ": row["VSSEQ"],
                "AVISITN": row["VISITNUM"],
                "ADT": row["VSDTC"].split("T", 1)[0],
                "AVAL": formatted(weight),
                "ANCHORWT": formatted(anchor),
                "BSA": formatted(bsa),
            }
        )

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with (OUTPUT_DIR / "advs.csv").open("w", newline="", encoding="utf-8") as target:
        writer = csv.DictWriter(target, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(result)


if __name__ == "__main__":
    main()
