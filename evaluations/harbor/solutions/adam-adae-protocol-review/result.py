# Reference solution for the yamaa benchmark adam-adae-protocol-review (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADAE from AE with protocol-review flags (ADaM)."""

import csv
from datetime import date, datetime

INPUT_CSV = "/app/input/ae.csv"
OUTPUT_CSV = "/app/output/adae.csv"

COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "ASTDT", "ASTDT2", "REVIEWFL"]


def parse_date(text):
    try:
        return date.fromisoformat(text)
    except ValueError:
        return None


def parse_datetime(text):
    try:
        return datetime.fromisoformat(text)
    except ValueError:
        return None


def in_window(aestdtc, aestdtm):
    if aestdtc:
        d = parse_date(aestdtc)
        if d is not None and d.year == 2025 and d.month == 1:
            return True
    if aestdtm:
        dt = parse_datetime(aestdtm)
        if dt is not None and dt >= datetime(2025, 2, 1, 9, 30):
            return True
    return False


def main():
    with open(INPUT_CSV, newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        aestdtc = (row.get("AESTDTC") or "").strip()
        aestdtm = (row.get("AESTDTM") or "").strip()
        aeterm = row.get("AETERM") or ""
        score_text = (row.get("SCORE") or "").strip()

        astdt = aestdtc
        astdt2 = ""
        if aestdtm:
            dt = parse_datetime(aestdtm)
            if dt is not None:
                astdt2 = dt.date().isoformat()

        window_ok = in_window(aestdtc, aestdtm)
        term_ok = aeterm.startswith("INF_")
        score_ok = False
        if score_text:
            try:
                score_ok = float(score_text) >= -1.5
            except ValueError:
                score_ok = False

        reviewfl = "Y" if (window_ok and term_ok and score_ok) else "N"

        out_rows.append(
            {
                "STUDYID": row.get("STUDYID", ""),
                "USUBJID": row.get("USUBJID", ""),
                "AESEQ": row.get("AESEQ", ""),
                "ASTDT": astdt,
                "ASTDT2": astdt2,
                "REVIEWFL": reviewfl,
            }
        )

    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
