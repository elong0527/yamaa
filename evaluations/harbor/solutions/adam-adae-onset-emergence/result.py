# Reference solution for the yamaa benchmark adam-adae-onset-emergence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Derive ADAE (one record per AE record) from ADSL and AE.

Treatment emergence is judged to the second.
"""

import csv
from datetime import datetime, timedelta
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

COLUMNS = [
    "STUDYID", "USUBJID", "AESEQ", "AETERM", "AESER", "ASTDTM", "TRTSDTM",
    "TRTEDTM", "EOTSTT", "TEENDDTM", "TRTEMFL", "AOCCFL",
]

ISO = "%Y-%m-%dT%H:%M:%S"


def parse_dtm(value):
    """ISO 8601 date-time to the second; a value without seconds is at second zero."""
    value = (value or "").strip()
    if not value:
        return None
    for fmt in (ISO, "%Y-%m-%dT%H:%M"):
        try:
            return datetime.strptime(value, fmt)
        except ValueError:
            pass
    return None


def fmt_dtm(value):
    return value.strftime(ISO) if value is not None else ""


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = {
        (row["STUDYID"], row["USUBJID"]): row
        for row in read_csv(INPUT / "adsl.csv")
    }
    ae = read_csv(INPUT / "ae.csv")

    records = []
    for row in ae:
        subject = adsl.get((row["STUDYID"], row["USUBJID"]), {})
        trtsdtm = parse_dtm(subject.get("TRTSDTM"))
        trtedtm = parse_dtm(subject.get("TRTEDTM"))
        eotstt = (subject.get("EOTSTT") or "").strip()
        aeser = (row.get("AESER") or "").strip()
        astdtm = parse_dtm(row.get("AESTDTC"))

        treated = trtsdtm is not None
        ongoing = eotstt == "ONGOING"

        teenddtm = None
        if treated and not ongoing and trtedtm is not None:
            days = 30 if aeser == "N" else 90
            teenddtm = trtedtm + timedelta(days=days)

        trtemfl = ""
        if treated:
            if astdtm is None:
                trtemfl = "Y"
            elif astdtm >= trtsdtm:
                if ongoing:
                    trtemfl = "Y"
                elif teenddtm is not None and astdtm <= teenddtm:
                    trtemfl = "Y"

        records.append({
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "AESEQ": int(row["AESEQ"]),
            "AETERM": row["AETERM"],
            "AESER": aeser,
            "ASTDTM": astdtm,
            "TRTSDTM": trtsdtm,
            "TRTEDTM": trtedtm,
            "EOTSTT": eotstt,
            "TEENDDTM": teenddtm,
            "TRTEMFL": trtemfl,
            "AOCCFL": "",
        })

    records.sort(key=lambda r: (r["STUDYID"], r["USUBJID"], r["AESEQ"]))

    # First treatment-emergent event per subject by ASTDTM, lower AESEQ on ties.
    first = {}
    for rec in records:
        if rec["TRTEMFL"] != "Y" or rec["ASTDTM"] is None:
            continue
        key = (rec["STUDYID"], rec["USUBJID"])
        best = first.get(key)
        if best is None or (rec["ASTDTM"], rec["AESEQ"]) < (best["ASTDTM"], best["AESEQ"]):
            first[key] = rec
    for rec in first.values():
        rec["AOCCFL"] = "Y"

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT / "adae.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(COLUMNS)
        for rec in records:
            writer.writerow([
                rec["STUDYID"], rec["USUBJID"], rec["AESEQ"], rec["AETERM"],
                rec["AESER"], fmt_dtm(rec["ASTDTM"]), fmt_dtm(rec["TRTSDTM"]),
                fmt_dtm(rec["TRTEDTM"]), rec["EOTSTT"], fmt_dtm(rec["TEENDDTM"]),
                rec["TRTEMFL"], rec["AOCCFL"],
            ])


if __name__ == "__main__":
    main()
