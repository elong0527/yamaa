# Reference solution for the yamaa benchmark adam-adsl-crossover-periods (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADSL (one record per subject) from DM and EX.

Derives the two crossover treatment periods per subject: earliest start
and latest end per epoch, the treatment of the earliest record in each
epoch, and the washout duration strictly between the periods.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "STUDYID", "USUBJID", "TR01SDT", "TR01EDT", "TR02SDT", "TR02EDT",
    "TRT01A", "TRT02A", "WASHDUR",
]


def parse_day(text):
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def read_rows(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def period(rows):
    """Return (start, end, treatment) for one subject's records in one epoch."""
    starts = [d for d in (parse_day(r["EXSTDTC"]) for r in rows) if d]
    ends = [d for d in (parse_day(r["EXENDTC"]) for r in rows) if d]
    start = min(starts) if starts else None
    end = max(ends) if ends else None
    treatment = ""
    if rows:
        def earliest_key(row):
            day = parse_day(row["EXSTDTC"])
            # A record with no start date counts as later than any with one.
            return (day is None, day or date.max, float(row["EXSEQ"]))

        treatment = min(rows, key=earliest_key)["EXTRT"]
    return start, end, treatment


def main():
    dm = read_rows(f"{INPUT_DIR}/dm.csv")
    ex = read_rows(f"{INPUT_DIR}/ex.csv")

    by_subject = {}
    for row in ex:
        by_subject.setdefault((row["STUDYID"], row["USUBJID"]), []).append(row)

    out = []
    for subject in dm:
        key = (subject["STUDYID"], subject["USUBJID"])
        records = by_subject.get(key, [])
        t1 = [r for r in records if r["EPOCH"] == "TREATMENT 1"]
        t2 = [r for r in records if r["EPOCH"] == "TREATMENT 2"]
        tr01sdt, tr01edt, trt01a = period(t1)
        tr02sdt, tr02edt, trt02a = period(t2)

        washdur = ""
        if tr01edt is not None and tr02sdt is not None:
            # Days strictly between the two dates, counting neither endpoint.
            washdur = str((tr02sdt - tr01edt).days - 1)

        def fmt(day):
            return day.isoformat() if day is not None else ""

        out.append({
            "STUDYID": subject["STUDYID"],
            "USUBJID": subject["USUBJID"],
            "TR01SDT": fmt(tr01sdt),
            "TR01EDT": fmt(tr01edt),
            "TR02SDT": fmt(tr02sdt),
            "TR02EDT": fmt(tr02edt),
            "TRT01A": trt01a,
            "TRT02A": trt02a,
            "WASHDUR": washdur,
        })

    with open(f"{OUTPUT_DIR}/adsl.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
