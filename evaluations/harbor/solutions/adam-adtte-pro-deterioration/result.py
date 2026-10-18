# Reference solution for the yamaa benchmark adam-adtte-pro-deterioration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
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
    return date.fromisoformat(text)


def read_csv(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def whole_months(start, end):
    months = (end.year - start.year) * 12 + (end.month - start.month)
    if end.day < start.day:
        months -= 1
    return months


def main():
    adsl = read_csv(INPUT_DIR + "/adsl.csv")
    ds = read_csv(INPUT_DIR + "/ds.csv")
    qs = read_csv(INPUT_DIR + "/qs.csv")

    # Earliest censoring record per subject.
    censor = {}
    for row in ds:
        if row["DSDECOD"] not in CENSOR_REASONS:
            continue
        when = parse_date(row["DSDTC"])
        if when is None:
            continue
        key = row["USUBJID"]
        if key not in censor or when < censor[key][0]:
            censor[key] = (when, row["DSDECOD"])

    # Assessments per subject, ordered by date then sequence.
    assessments = {}
    for row in qs:
        when = parse_date(row["ADT"])
        if when is None:
            continue
        score = row["AVAL"].strip()
        assessments.setdefault(row["USUBJID"], []).append(
            (when, int(row["QSSEQ"]), float(score) if score else None)
        )
    for rows in assessments.values():
        rows.sort(key=lambda item: (item[0], item[1]))

    out_rows = []
    for subj in adsl:
        usubjid = subj["USUBJID"]
        startdt = parse_date(subj["RANDDT"])
        dthdt = parse_date(subj["DTHDT"])
        censor_date, censor_reason = censor.get(usubjid, (None, None))

        # Deterioration: earliest assessment after the earliest one whose
        # score is at least 10 points below the earliest score.
        deterioration = None
        rows = assessments.get(usubjid, [])
        if rows:
            base_score = rows[0][2]
            for when, seq, score in rows[1:]:
                if (
                    base_score is not None
                    and score is not None
                    and base_score - score >= 10
                ):
                    deterioration = (when, seq)
                    break

        det_ok = deterioration is not None and (
            censor_date is None or deterioration[0] <= censor_date
        )
        dth_ok = dthdt is not None and (
            censor_date is None or dthdt <= censor_date
        )

        adt = None
        cnsr = 1
        evntdesc = "CENSORED"
        cnsdtdsc = censor_reason if censor_reason else "STUDY COMPLETION"
        srcdom = "ADSL"
        srcvar = "RANDDT"
        srcseq = ""

        if det_ok and (not dth_ok or deterioration[0] <= dthdt):
            adt = deterioration[0]
            cnsr = 0
            evntdesc = "PRO DETERIORATION"
            cnsdtdsc = ""
            srcdom, srcvar, srcseq = "QS", "ADT", str(deterioration[1])
        elif dth_ok:
            adt = dthdt
            cnsr = 0
            evntdesc = "DEATH"
            cnsdtdsc = ""
            srcdom, srcvar, srcseq = "ADSL", "DTHDT", ""
        else:
            supporting = None
            for when, seq, _score in rows:
                if censor_date is None or when <= censor_date:
                    supporting = (when, seq)
            if supporting is not None:
                adt = supporting[0]
                srcdom, srcvar, srcseq = "QS", "ADT", str(supporting[1])
            else:
                adt = startdt
                srcdom, srcvar, srcseq = "ADSL", "RANDDT", ""

        aval = whole_months(startdt, adt) if startdt and adt else ""
        out_rows.append(
            {
                "STUDYID": subj["STUDYID"],
                "USUBJID": usubjid,
                "PARAMCD": "TTDGHS",
                "PARAM": "Time to Deterioration in Global Health Status",
                "STARTDT": startdt.isoformat() if startdt else "",
                "ADT": adt.isoformat() if adt else "",
                "AVAL": aval,
                "CNSR": cnsr,
                "EVNTDESC": evntdesc,
                "CNSDTDSC": cnsdtdsc,
                "SRCDOM": srcdom,
                "SRCVAR": srcvar,
                "SRCSEQ": srcseq,
            }
        )

    columns = [
        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "STARTDT", "ADT",
        "AVAL", "CNSR", "EVNTDESC", "CNSDTDSC", "SRCDOM", "SRCVAR",
        "SRCSEQ",
    ]
    with open(OUTPUT_DIR + "/adtte.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
