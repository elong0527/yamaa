# Reference solution for the yamaa benchmark adam-adsl-alive-date (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Derive ADSL (STUDYID, USUBJID, LSTALVDT, DTHFL, DTHDT, DTHDTF) from SDTM.

Reads the SDTM datasets from /app/input and writes /app/output/adsl.csv.
"""

import csv
import re
from datetime import date

INPUT = "/app/input"
OUTPUT = "/app/output"

DATE_RE = re.compile(r"^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?(?:[T ].*)?$")


def read(name):
    with open(f"{INPUT}/{name}.csv", newline="", encoding="utf-8-sig") as handle:
        return list(csv.DictReader(handle))


def parse_iso(text):
    """Return (year, month, day) with None for components not collected."""
    match = DATE_RE.match((text or "").strip())
    if not match:
        return None
    year, month, day = match.groups()
    return int(year), (int(month) if month else None), (int(day) if day else None)


def complete(text):
    """Complete a date to a day: a year and month to the 1st, a year to 1 Jan."""
    parts = parse_iso(text)
    if parts is None:
        return None
    year, month, day = parts
    try:
        return date(year, month or 1, day or 1)
    except ValueError:
        return None


def norm(value):
    return (value or "").strip().upper()


def main():
    dm = read("dm")
    alive = {}

    def add(usubjid, text):
        day = complete(text)
        if day is None:
            return
        current = alive.get(usubjid)
        if current is None or day > current:
            alive[usubjid] = day

    for row in read("ex"):
        add(row["USUBJID"], row.get("EXSTDTC"))
        add(row["USUBJID"], row.get("EXENDTC"))
    for row in read("ae"):
        add(row["USUBJID"], row.get("AESTDTC"))
        add(row["USUBJID"], row.get("AEENDTC"))
    for row in read("ds"):
        if norm(row.get("DSDECOD")) in {"DEATH", "LOST TO FOLLOW-UP"}:
            continue
        add(row["USUBJID"], row.get("DSSTDTC"))
    for row in read("sv"):
        add(row["USUBJID"], row.get("SVSTDTC"))
    for row in read("vs"):
        add(row["USUBJID"], row.get("VSDTC"))
    for row in read("ss"):
        if norm(row.get("SSSTRESC")) == "ALIVE":
            add(row["USUBJID"], row.get("SSDTC"))

    out = []
    seen = set()
    for row in dm:
        usubjid = row["USUBJID"]
        if usubjid in seen:
            continue
        seen.add(usubjid)

        last_alive = alive.get(usubjid)
        dthdtc = (row.get("DTHDTC") or "").strip()
        parts = parse_iso(dthdtc) if dthdtc else None
        dead = norm(row.get("DTHFL")) == "Y" or parts is not None

        dthdt = None
        dthdtf = ""
        if parts is not None:
            year, month, day = parts
            base = complete(dthdtc)
            if day is not None:
                dthdt = base
            else:
                dthdt = base
                dthdtf = "D" if month is not None else "M"
                if last_alive is not None and last_alive > base:
                    same_period = last_alive.year == year and (
                        month is None or last_alive.month == month
                    )
                    if same_period:
                        dthdt = last_alive

        lstalvdt = last_alive
        if lstalvdt is not None and dthdt is not None and lstalvdt > dthdt:
            lstalvdt = dthdt

        out.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": usubjid,
                "LSTALVDT": lstalvdt.isoformat() if lstalvdt else "",
                "DTHFL": "Y" if dead else "",
                "DTHDT": dthdt.isoformat() if dthdt else "",
                "DTHDTF": dthdtf,
            }
        )

    columns = ["STUDYID", "USUBJID", "LSTALVDT", "DTHFL", "DTHDT", "DTHDTF"]
    with open(f"{OUTPUT}/adsl.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, lineterminator="\n")
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
