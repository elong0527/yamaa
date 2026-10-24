# Reference solution for the yamaa benchmark adam-adcm-free-text-start-date (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADCM from CM, reading a free-text start date in five spellings."""

import csv
import re
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

MONTHS = [
    "JAN",
    "FEB",
    "MAR",
    "APR",
    "MAY",
    "JUN",
    "JUL",
    "AUG",
    "SEP",
    "OCT",
    "NOV",
    "DEC",
]


def month_number(abbreviation):
    """The month a three-letter abbreviation names, in any letter case."""
    upper = abbreviation.upper()
    return MONTHS.index(upper) + 1 if upper in MONTHS else None


def iso_date(year, month, day):
    return date(int(year), int(month), int(day)).isoformat()


def read_start(text):
    """The start as ISO 8601 text at the precision collected, or ""."""
    if not text:
        return ""
    if re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", text):
        return iso_date(text[:4], text[5:7], text[8:])
    match = re.fullmatch(r"([0-9]{2})([A-Za-z]{3})([0-9]{4})", text)
    if match and month_number(match[2]):
        return iso_date(match[3], month_number(match[2]), match[1])
    match = re.fullmatch(r"([0-9]{2})/([0-9]{2})/([0-9]{4})", text)
    if match:
        # The month comes first.
        return iso_date(match[3], match[1], match[2])
    if re.fullmatch(r"[0-9]{8}", text):
        return iso_date(text[:4], text[4:6], text[6:])
    match = re.fullmatch(r"([A-Za-z]{3})([0-9]{4})", text)
    if match and month_number(match[1]):
        # A month alone.
        return f"{match[2]}-{month_number(match[1]):02d}"
    raise ValueError(f"CMSTDAT {text!r} is in none of the five spellings")


def main():
    with open(f"{INPUT_DIR}/cm.csv", newline="") as handle:
        cm = list(csv.DictReader(handle))
    fieldnames = ["STUDYID", "USUBJID", "CMSEQ", "CMTRT", "CMSTDAT", "ASTDTC", "ASTDT"]
    with open(f"{OUTPUT_DIR}/adcm.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in cm:
            astdtc = read_start(row["CMSTDAT"])
            # A month alone starts on its first day.
            astdt = astdtc + "-01" if len(astdtc) == 7 else astdtc
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "CMSEQ": row["CMSEQ"],
                    "CMTRT": row["CMTRT"],
                    "CMSTDAT": row["CMSTDAT"],
                    "ASTDTC": astdtc,
                    "ASTDT": astdt,
                }
            )


if __name__ == "__main__":
    main()
