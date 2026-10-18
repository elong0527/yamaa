# Reference solution for the yamaa benchmark adam-adex-infusion-duration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADEX (one record per infusion) from EX.

PARAMCD is "INFDUR" and PARAM is "Infusion Duration (min)" on every
record; AVALU is "min" on every record.

CALCDUR is the elapsed time in minutes from EXSTDTC to EXENDTC, with
seconds kept as a fraction of a minute and nothing added or rounded.
It has no value when EXSTDTC or EXENDTC was collected as a date
without a time of day.

AVAL is the collected duration in minutes when EXDUR holds a readable
fixed-length ISO 8601 duration; otherwise AVAL is CALCDUR. Text that
is not such a duration (for example one without its leading P) counts
as not collected, and a readable collected duration always wins even
when it disagrees with the start and end times. AVAL has no value when
neither gives a value; every infusion keeps its record.
"""

import csv
import re
from datetime import datetime

INPUT_EX = "/app/input/ex.csv"
OUTPUT_ADEX = "/app/output/adex.csv"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "EXSEQ",
    "EXSTDTC",
    "EXENDTC",
    "EXDUR",
    "PARAMCD",
    "PARAM",
    "AVAL",
    "AVALU",
    "CALCDUR",
]

# A fixed-length ISO 8601 duration: P[nD][T[nH][nM][nS]] with at least
# one component. Years, months and weeks are not fixed length, so a
# duration using them is not readable here.
DURATION_RE = re.compile(
    r"^P(?:(?P<days>\d+(?:\.\d+)?)D)?"
    r"(?:T(?:(?P<hours>\d+(?:\.\d+)?)H)?"
    r"(?:(?P<minutes>\d+(?:\.\d+)?)M)?"
    r"(?:(?P<seconds>\d+(?:\.\d+)?)S)?)?$"
)


def parse_duration_minutes(text):
    """Minutes in a fixed-length ISO 8601 duration, or None."""
    if not text:
        return None
    match = DURATION_RE.match(text.strip())
    if match is None:
        return None
    parts = match.groupdict()
    if all(value is None for value in parts.values()):
        return None
    total = 0.0
    if parts["days"] is not None:
        total += float(parts["days"]) * 1440.0
    if parts["hours"] is not None:
        total += float(parts["hours"]) * 60.0
    if parts["minutes"] is not None:
        total += float(parts["minutes"])
    if parts["seconds"] is not None:
        total += float(parts["seconds"]) / 60.0
    return total


def parse_datetime(text):
    """A datetime for an ISO 8601 value that carries a time of day,
    or None when the value is a date without a time (or missing)."""
    if not text:
        return None
    text = text.strip()
    if "T" not in text:
        return None
    return datetime.fromisoformat(text)


def calc_duration_minutes(start_text, end_text):
    """Elapsed minutes from start to end, or None when either value
    was collected as a date without a time of day."""
    start = parse_datetime(start_text)
    end = parse_datetime(end_text)
    if start is None or end is None:
        return None
    return (end - start).total_seconds() / 60.0


def format_number(value):
    """A number as text: whole minutes without a decimal point,
    otherwise the exact value (no rounding beyond float repr)."""
    if value is None:
        return ""
    if float(value).is_integer():
        return str(int(value))
    return repr(float(value))


def main():
    with open(INPUT_EX, newline="", encoding="utf-8") as handle:
        ex_rows = list(csv.DictReader(handle))

    out_rows = []
    for row in ex_rows:
        calcdur = calc_duration_minutes(row["EXSTDTC"], row["EXENDTC"])
        collected = parse_duration_minutes(row["EXDUR"])
        aval = collected if collected is not None else calcdur
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "EXSEQ": row["EXSEQ"],
                "EXSTDTC": row["EXSTDTC"],
                "EXENDTC": row["EXENDTC"],
                "EXDUR": row["EXDUR"],
                "PARAMCD": "INFDUR",
                "PARAM": "Infusion Duration (min)",
                "AVAL": format_number(aval),
                "AVALU": "min",
                "CALCDUR": format_number(calcdur),
            }
        )

    with open(OUTPUT_ADEX, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
