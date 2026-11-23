# Reference solution for the yamaa benchmark adam-advs-crossover-period-records (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from datetime import datetime

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    return datetime.strptime(text, "%Y-%m-%d").date()


def parse_dtc(text):
    """Return (date, datetime or None) for an ISO 8601 collection value.

    A date-only value yields a date with no datetime; a value carrying a
    time yields both its date and its datetime.
    """
    text = (text or "").strip()
    if not text:
        return None, None
    if "T" not in text:
        return parse_date(text), None
    for fmt in ("%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"):
        try:
            moment = datetime.strptime(text, fmt)
            return moment.date(), moment
        except ValueError:
            continue
    raise ValueError("unrecognised datetime: %r" % text)


def fmt_date(d):
    return d.isoformat() if d is not None else ""


def fmt_dtm(m):
    return m.strftime("%Y-%m-%dT%H:%M:%S") if m is not None else ""


def main():
    with open(INPUT_DIR + "/adsl.csv", newline="") as f:
        adsl = {row["USUBJID"]: row for row in csv.DictReader(f)}
    with open(INPUT_DIR + "/vs.csv", newline="") as f:
        vs_rows = list(csv.DictReader(f))

    out_rows = []
    for row in vs_rows:
        subj = adsl[row["USUBJID"]]
        # The subject's periods with a start and an end date, in order:
        # (number, start date, start datetime or None, end date, end
        # datetime or None, actual treatment).
        periods = []
        for num in (1, 2):
            prefix = "AP%02d" % num
            start_date = parse_date(subj.get(prefix + "SDT"))
            end_date = parse_date(subj.get(prefix + "EDT"))
            if start_date is None or end_date is None:
                continue
            _, start_dtm = parse_dtc(subj.get(prefix + "SDTM"))
            _, end_dtm = parse_dtc(subj.get(prefix + "EDTM"))
            trt = subj.get("TRT%02dA" % num) or ""
            periods.append((num, start_date, start_dtm, end_date, end_dtm, trt))

        coll_date, coll_dtm = parse_dtc(row.get("VSDTC"))
        aperiod = ""
        trta = ""
        if coll_date is not None:
            # Each limit is compared by time when both the record and the
            # limit have one, and by date otherwise. A later period that
            # also holds the record wins.
            for num, start_date, start_dtm, end_date, end_dtm, trt in periods:
                if coll_dtm is not None and start_dtm is not None:
                    after_start = start_dtm <= coll_dtm
                else:
                    after_start = start_date <= coll_date
                if coll_dtm is not None and end_dtm is not None:
                    before_end = coll_dtm <= end_dtm
                else:
                    before_end = coll_date <= end_date
                if after_start and before_end:
                    aperiod = num
                    trta = trt
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "VSSEQ": row["VSSEQ"],
                "PARAMCD": row["VSTESTCD"],
                "AVAL": row["VSSTRESN"],
                "ADT": fmt_date(coll_date),
                "ADTM": fmt_dtm(coll_dtm),
                "APERIOD": aperiod,
                "TRTA": trta,
            }
        )

    fieldnames = [
        "STUDYID",
        "USUBJID",
        "VSSEQ",
        "PARAMCD",
        "AVAL",
        "ADT",
        "ADTM",
        "APERIOD",
        "TRTA",
    ]
    with open(OUTPUT_DIR + "/advs.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
