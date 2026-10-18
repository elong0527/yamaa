# Reference solution for the yamaa benchmark adam-adtte-first-ae (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADTTE dataset (time to first adverse event) from ADAE and ADSL.

One record per subject (from ADSL). The first adverse event with a usable
onset date (ASTDT) is the event; subjects without a dated event are
censored at their end-of-study date (EOSDT).
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_date(value):
    """Parse an ISO date string, returning None for blank values."""
    value = (value or "").strip()
    if not value:
        return None
    return date.fromisoformat(value)


def fmt_date(value):
    return value.isoformat() if value is not None else ""


def read_rows(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = read_rows(f"{INPUT_DIR}/adsl.csv")
    adae = read_rows(f"{INPUT_DIR}/adae.csv")

    events_by_subject = {}
    for row in adae:
        events_by_subject.setdefault(row["USUBJID"], []).append(row)

    records = []
    for subj in adsl:
        startdt = parse_date(subj.get("TRTSDT"))
        eosdt = parse_date(subj.get("EOSDT"))

        dated_events = []
        for event in events_by_subject.get(subj["USUBJID"], []):
            onset = parse_date(event.get("ASTDT"))
            if onset is not None:
                dated_events.append((onset, int(event["AESEQ"])))

        if dated_events:
            # Earliest onset date; lowest sequence number breaks ties.
            adt, srcseq = min(dated_events)
            cnsr, evntdesc, srcdom, srcvar = 0, "AE", "ADAE", "ASTDT"
            srcseq_text = str(srcseq)
        else:
            adt = eosdt
            cnsr, evntdesc, srcdom, srcvar = 1, "END OF STUDY", "ADSL", "EOSDT"
            srcseq_text = ""

        # A date before the treatment start is moved up to the treatment
        # start; the record keeps its event or censoring status.
        if adt is not None and startdt is not None and adt < startdt:
            adt = startdt

        if adt is not None and startdt is not None:
            aval = str((adt - startdt).days + 1)
        else:
            aval = ""

        records.append(
            {
                "STUDYID": subj["STUDYID"],
                "USUBJID": subj["USUBJID"],
                "PARAMCD": "TTAE",
                "PARAM": "Time to First Adverse Event",
                "STARTDT": fmt_date(startdt),
                "ADT": fmt_date(adt),
                "AVAL": aval,
                "CNSR": str(cnsr),
                "EVNTDESC": evntdesc,
                "SRCDOM": srcdom,
                "SRCVAR": srcvar,
                "SRCSEQ": srcseq_text,
            }
        )

    fieldnames = [
        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "STARTDT", "ADT",
        "AVAL", "CNSR", "EVNTDESC", "SRCDOM", "SRCVAR", "SRCSEQ",
    ]
    with open(f"{OUTPUT_DIR}/adtte.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
