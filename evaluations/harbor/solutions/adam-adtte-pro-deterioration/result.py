# Reference solution for the yamaa benchmark adam-adtte-pro-deterioration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADTTE dataset (time to deterioration in global health status).

Reads ADSL, DS and QS from /app/input and writes /app/output/adtte.csv
with one record per subject.
"""

import csv
import os
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

CENSOR_REASONS = {
    "DISEASE PROGRESSION",
    "STUDY DISCONTINUATION",
    "WITHDRAWAL OF CONSENT",
}


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text[:10])


def fmt(d):
    return d.isoformat() if d is not None else ""


def whole_months(start, end):
    """Whole calendar months from start to end."""
    months = (end.year - start.year) * 12 + (end.month - start.month)
    if end.day < start.day:
        months -= 1
    return months


def read_csv(name):
    path = os.path.join(INPUT_DIR, name)
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def to_int(text, default=0):
    try:
        return int(float(text))
    except (TypeError, ValueError):
        return default


def main():
    adsl = read_csv("adsl.csv")
    ds = read_csv("ds.csv")
    qs = read_csv("qs.csv")

    ds_by_subject = {}
    for row in ds:
        ds_by_subject.setdefault(row["USUBJID"], []).append(row)

    qs_by_subject = {}
    for row in qs:
        qs_by_subject.setdefault(row["USUBJID"], []).append(row)

    out_rows = []
    for subj in adsl:
        usubjid = subj["USUBJID"]
        randdt = parse_date(subj.get("RANDDT"))
        dthdt = parse_date(subj.get("DTHDT"))

        # Assessments with both a date and a score, in date/sequence order.
        assessments = []
        for row in qs_by_subject.get(usubjid, []):
            adt = parse_date(row.get("ADT"))
            score_text = (row.get("AVAL") or "").strip()
            if adt is None or not score_text:
                continue
            assessments.append(
                {
                    "date": adt,
                    "score": float(score_text),
                    "seq": to_int(row.get("QSSEQ")),
                }
            )
        assessments.sort(key=lambda a: (a["date"], a["seq"]))

        # Deterioration: an assessment dated after the earliest assessment
        # whose score has fallen at least 10 points below that earliest
        # assessment's score; the earliest such assessment gives the date.
        deterioration = None
        if assessments:
            baseline = assessments[0]
            for a in assessments[1:]:
                if a["date"] > baseline["date"] and (
                    baseline["score"] - a["score"] >= 10
                ):
                    deterioration = a
                    break

        # First censoring reason: earliest disposition record naming one of
        # the censoring reasons (never an event).
        censor_date = None
        censor_reason = ""
        records = []
        for row in ds_by_subject.get(usubjid, []):
            if (row.get("DSDECOD") or "").strip() in CENSOR_REASONS:
                records.append(
                    {
                        "date": parse_date(row.get("DSDTC")),
                        "seq": to_int(row.get("DSSEQ")),
                        "reason": (row.get("DSDECOD") or "").strip(),
                    }
                )
        records.sort(
            key=lambda r: (r["date"] is None, r["date"] or date.min, r["seq"])
        )
        if records:
            censor_date = records[0]["date"]
            censor_reason = records[0]["reason"]

        def not_after_censor(d):
            return d is not None and (censor_date is None or d <= censor_date)

        # Event selection: deterioration and death compete on date, with a
        # deterioration winning a same-date tie; each must not be dated
        # after the first censoring reason.
        event = None  # (kind, date, seq)
        candidates = []
        if deterioration is not None and not_after_censor(deterioration["date"]):
            candidates.append(("deterioration", deterioration["date"], 0))
        if dthdt is not None and not_after_censor(dthdt):
            candidates.append(("death", dthdt, 1))
        if candidates:
            candidates.sort(key=lambda c: (c[1], c[2]))
            kind, event_date, _ = candidates[0]
            if kind == "deterioration":
                event = ("deterioration", event_date, deterioration["seq"])
            else:
                event = ("death", event_date, None)

        if event is not None:
            kind, adt, seq = event
            cnsr = 0
            cnsdtdsc = ""
            if kind == "deterioration":
                evntdesc = "PRO DETERIORATION"
                srcdom, srcvar, srcseq = "QS", "ADT", str(seq)
            else:
                evntdesc = "DEATH"
                srcdom, srcvar, srcseq = "ADSL", "DTHDT", ""
        else:
            cnsr = 1
            evntdesc = "CENSORED"
            cnsdtdsc = censor_reason if censor_reason else "STUDY COMPLETION"
            qualifying = [
                a
                for a in assessments
                if censor_date is None or a["date"] <= censor_date
            ]
            if qualifying:
                last = max(qualifying, key=lambda a: (a["date"], a["seq"]))
                adt = last["date"]
                srcdom, srcvar, srcseq = "QS", "ADT", str(last["seq"])
            else:
                adt = randdt
                srcdom, srcvar, srcseq = "ADSL", "RANDDT", ""

        out_rows.append(
            {
                "STUDYID": subj["STUDYID"],
                "USUBJID": usubjid,
                "PARAMCD": "TTDGHS",
                "PARAM": "Time to Deterioration in Global Health Status",
                "STARTDT": fmt(randdt),
                "ADT": fmt(adt),
                "AVAL": str(whole_months(randdt, adt)),
                "CNSR": str(cnsr),
                "EVNTDESC": evntdesc,
                "CNSDTDSC": cnsdtdsc,
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
        "CNSDTDSC",
        "SRCDOM",
        "SRCVAR",
        "SRCSEQ",
    ]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(
        os.path.join(OUTPUT_DIR, "adtte.csv"), "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
