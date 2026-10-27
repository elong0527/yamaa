"""Create the event-level ADAE start datetime analysis variables."""

import csv
import re
from datetime import datetime
from pathlib import Path


COLUMNS = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AETERM",
    "ASTDTM",
    "ASTDT",
    "ASTTM",
    "ASTTMF",
]
START_PATTERN = re.compile(
    r"(\d{4})-(\d{2})-(\d{2})(?:T(\d{2})(?::(\d{2})(?::(\d{2}))?)?)?"
)


def derive_start(value):
    """Impute only uncollected time components of a complete calendar date."""
    match = START_PATTERN.fullmatch((value or "").strip())
    if match is None:
        return {column: "" for column in COLUMNS[4:]}

    year, month, day, hour, minute, second = match.groups()
    try:
        start = datetime(
            int(year),
            int(month),
            int(day),
            int(hour or 0),
            int(minute or 0),
            int(second or 0),
        )
    except ValueError:
        return {column: "" for column in COLUMNS[4:]}

    flag = (
        "H"
        if hour is None
        else "M"
        if minute is None
        else "S"
        if second is None
        else ""
    )
    return {
        "ASTDTM": start.isoformat(timespec="seconds"),
        "ASTDT": start.date().isoformat(),
        "ASTTM": start.hour * 3600 + start.minute * 60 + start.second,
        "ASTTMF": flag,
    }


def main():
    output_path = Path("/app/output/adae.csv")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with Path("/app/input/ae.csv").open(newline="", encoding="utf-8-sig") as source:
        with output_path.open("w", newline="", encoding="utf-8") as target:
            writer = csv.DictWriter(target, fieldnames=COLUMNS)
            writer.writeheader()
            for event in csv.DictReader(source):
                row = {column: event.get(column, "") for column in COLUMNS[:4]}
                row.update(derive_start(event.get("AESTDTC")))
                writer.writerow(row)


if __name__ == "__main__":
    main()
