# Reference solution for the yamaa benchmark adam-adae-post-dose-window (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""ADAE post-dose window: flag events starting within 24 hours of a dose."""

import csv
from datetime import datetime, timedelta
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")


def parse_dtc(text):
    """Parse an ISO datetime that may be a date alone or lack seconds.

    Returns (datetime or None, has_time). A time collected without
    seconds counts as second zero.
    """
    text = (text or "").strip()
    if not text:
        return None, False
    if "T" not in text:
        return datetime.strptime(text, "%Y-%m-%d"), False
    for fmt in ("%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"):
        try:
            return datetime.strptime(text, fmt), True
        except ValueError:
            continue
    raise ValueError(f"unrecognised datetime: {text!r}")


def main():
    with open(INPUT / "ae.csv", newline="") as fh:
        ae_rows = list(csv.DictReader(fh))
    with open(INPUT / "ex.csv", newline="") as fh:
        ex_rows = list(csv.DictReader(fh))

    doses = {}
    for row in ex_rows:
        start, _ = parse_dtc(row["EXSTDTC"])
        if start is not None:
            doses.setdefault(row["USUBJID"], []).append(start)
    for subject_doses in doses.values():
        subject_doses.sort()

    out_rows = []
    for row in ae_rows:
        onset, has_time = parse_dtc(row["AESTDTC"])
        astdt = onset.strftime("%Y-%m-%d") if onset is not None else ""
        astdtm = row["AESTDTC"].strip() if has_time else ""
        dose24fl = ""
        dosenum = ""
        if onset is not None and has_time:
            subject_doses = doses.get(row["USUBJID"], [])
            if subject_doses and onset >= subject_doses[0]:
                holding = [
                    number
                    for number, start in enumerate(subject_doses, start=1)
                    if start <= onset < start + timedelta(hours=24)
                ]
                if holding:
                    dose24fl = "Y"
                    dosenum = str(max(holding))
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AESEQ": row["AESEQ"],
                "AEDECOD": row["AEDECOD"],
                "ASTDT": astdt,
                "ASTDTM": astdtm,
                "DOSE24FL": dose24fl,
                "DOSENUM": dosenum,
            }
        )

    OUTPUT.mkdir(parents=True, exist_ok=True)
    columns = [
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AEDECOD",
        "ASTDT",
        "ASTDTM",
        "DOSE24FL",
        "DOSENUM",
    ]
    with open(OUTPUT / "adae.csv", "w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
