# Reference solution for the yamaa benchmark adam-adtte-pfs (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADTTE (PFS) from ADSL, DS, and RS per the task instruction.

Reads /app/input/{adsl,ds,rs}.csv and writes /app/output/adtte.csv with
one record per subject.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adtte.csv"


def parse_date(text):
    """Return a date for an ISO date string, or None when blank."""
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def read_rows(name):
    with open(f"{INPUT_DIR}/{name}", newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = read_rows("adsl.csv")
    ds = read_rows("ds.csv")
    rs = read_rows("rs.csv")

    # Per-subject new anti-cancer therapy start: only dates on or before
    # it are usable for events and censoring.
    ntx_by_subject = {}
    for subj in adsl:
        key = (subj["STUDYID"], subj["USUBJID"])
        ntx_by_subject[key] = parse_date(subj["NTXSTDT"])

    def usable(key, day):
        ntx = ntx_by_subject.get(key)
        return ntx is None or day <= ntx

    deaths = {}
    for row in ds:
        if row["DSDECOD"] != "DEATH":
            continue
        dthdt = parse_date(row["DSDTC"])
        if dthdt is None:
            continue
        key = (row["STUDYID"], row["USUBJID"])
        if not usable(key, dthdt):
            continue
        candidate = (dthdt, int(row["DSSEQ"]))
        if key not in deaths or candidate < deaths[key]:
            deaths[key] = candidate

    progressions = {}
    last_adequate = {}
    for row in rs:
        if row["RSTESTCD"] != "OVRLRESP" or row["ADEQFL"] != "Y":
            continue
        adt = parse_date(row["RSDTC"])
        if adt is None:
            continue
        key = (row["STUDYID"], row["USUBJID"])
        if not usable(key, adt):
            continue
        seq = int(row["RSSEQ"])
        if row["RSSTRESC"] == "PD":
            candidate = (adt, seq)
            if key not in progressions or candidate < progressions[key]:
                progressions[key] = candidate
        latest = last_adequate.get(key)
        if latest is None or (adt, seq) > latest:
            last_adequate[key] = (adt, seq)

    out_rows = []
    for subj in adsl:
        studyid = subj["STUDYID"]
        usubjid = subj["USUBJID"]
        key = (studyid, usubjid)
        startdt = parse_date(subj["RANDDT"])

        prog = progressions.get(key)
        death = deaths.get(key)

        srcdom = srcvar = srcseq = ""
        if prog is not None and (death is None or prog[0] <= death[0]):
            adt = prog[0]
            cnsr = 0
            evntdesc = "DISEASE PROGRESSION"
            srcdom, srcvar, srcseq = "RS", "RSDTC", prog[1]
        elif death is not None:
            adt = death[0]
            cnsr = 0
            evntdesc = "DEATH"
            srcdom, srcvar, srcseq = "DS", "DSDTC", death[1]
        else:
            cnsr = 1
            evntdesc = "CENSORED"
            last = last_adequate.get(key)
            if last is None:
                # No usable adequate assessment: censored at randomization.
                adt = startdt
            else:
                adt = last[0]
                srcdom, srcvar, srcseq = "RS", "RSDTC", last[1]

        aval = (adt - startdt).days + 1

        out_rows.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "PARAMCD": "PFS",
                "PARAM": "Progression-Free Survival",
                "STARTDT": startdt.isoformat(),
                "ADT": adt.isoformat(),
                "AVAL": aval,
                "CNSR": cnsr,
                "EVNTDESC": evntdesc,
                "SRCDOM": srcdom,
                "SRCVAR": srcvar,
                "SRCSEQ": srcseq,
            }
        )

    fieldnames = [
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "STARTDT",
        "ADT",
        "AVAL",
        "CNSR",
        "EVNTDESC",
        "SRCDOM",
        "SRCVAR",
        "SRCSEQ",
    ]
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
