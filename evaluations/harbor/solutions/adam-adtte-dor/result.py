# Reference solution for the yamaa benchmark adam-adtte-dor (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADTTE dataset (Duration of Response) from ADRS_RAW, ADSL and DS.

One record per subject whose ADSL RESPDT is not blank.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def read_csv(name):
    with open(f"{INPUT_DIR}/{name}", newline="") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = read_csv("adsl.csv")
    adrs = read_csv("adrs_raw.csv")
    ds = read_csv("ds.csv")

    adrs_by_subject = {}
    for row in adrs:
        adrs_by_subject.setdefault(row["USUBJID"], []).append(row)

    ds_by_subject = {}
    for row in ds:
        ds_by_subject.setdefault(row["USUBJID"], []).append(row)

    out_rows = []
    for subj in adsl:
        respdt = parse_date(subj["RESPDT"])
        if respdt is None:
            continue
        usubjid = subj["USUBJID"]
        therapy_start = parse_date(subj["NACTDT"])

        assessments = adrs_by_subject.get(usubjid, [])

        # Progression: earliest dated assessment with response "PD";
        # ties on that date go to the lowest ASEQ.
        pd_records = [
            r for r in assessments
            if r["AVALC"].strip() == "PD" and parse_date(r["ADT"]) is not None
        ]
        prog_date = None
        prog_seq = None
        if pd_records:
            prog_date = min(parse_date(r["ADT"]) for r in pd_records)
            prog_seq = min(
                int(r["ASEQ"]) for r in pd_records
                if parse_date(r["ADT"]) == prog_date
            )

        # Death: earliest DS record with a "DEATH" term.
        death_records = [
            r for r in ds_by_subject.get(usubjid, [])
            if r["DSDECOD"].strip() == "DEATH" and parse_date(r["DSSTDTC"]) is not None
        ]
        death_date = None
        death_seq = None
        if death_records:
            death_date = min(parse_date(r["DSSTDTC"]) for r in death_records)
            death_seq = min(
                int(r["DSSEQ"]) for r in death_records
                if parse_date(r["DSSTDTC"]) == death_date
            )

        # Event date: earlier of progression and death.
        event_date = None
        event_kind = None
        if prog_date is not None and (death_date is None or prog_date <= death_date):
            event_date, event_kind = prog_date, "PD"
        elif death_date is not None:
            event_date, event_kind = death_date, "DEATH"

        # The event counts only when it does not fall after the therapy
        # start date; an event on the therapy start date itself counts.
        counted = event_date is not None and (
            therapy_start is None or event_date <= therapy_start
        )

        # Last valid tumour assessment: latest dated assessment whose
        # response is not "NE"; ties on that date go to the highest ASEQ.
        valid_records = [
            r for r in assessments
            if r["AVALC"].strip() != "NE" and parse_date(r["ADT"]) is not None
        ]
        last_valid_date = None
        last_valid_seq = None
        if valid_records:
            last_valid_date = max(parse_date(r["ADT"]) for r in valid_records)
            last_valid_seq = max(
                int(r["ASEQ"]) for r in valid_records
                if parse_date(r["ADT"]) == last_valid_date
            )

        if counted:
            adt = event_date
            cnsr = 0
            if event_kind == "PD":
                evntdesc = "DISEASE PROGRESSION"
                cnsdtdsc = ""
                srcdom = "ADRS"
                srcvar = "ADT"
                srcseq = prog_seq
            else:
                evntdesc = "DEATH"
                cnsdtdsc = ""
                srcdom = "DS"
                srcvar = "DSSTDTC"
                srcseq = death_seq
        else:
            cnsr = 1
            evntdesc = "CENSORED"
            candidates = [
                d for d in (last_valid_date, therapy_start) if d is not None
            ]
            adt = min(candidates) if candidates else None
            if adt is not None and adt == last_valid_date:
                cnsdtdsc = "LAST TUMOUR ASSESSMENT"
                srcdom = "ADRS"
                srcvar = "ADT"
                srcseq = last_valid_seq
            else:
                cnsdtdsc = "START OF NEW ANTI-CANCER THERAPY"
                srcdom = "ADSL"
                srcvar = "NACTDT"
                srcseq = None

        aval = ""
        if adt is not None:
            aval = max(1, (adt - respdt).days + 1)

        out_rows.append({
            "STUDYID": subj["STUDYID"],
            "USUBJID": usubjid,
            "PARAMCD": "DOR",
            "PARAM": "Duration of Response",
            "STARTDT": respdt.isoformat(),
            "ADT": adt.isoformat() if adt is not None else "",
            "AVAL": aval,
            "CNSR": cnsr,
            "EVNTDESC": evntdesc,
            "CNSDTDSC": cnsdtdsc,
            "SRCDOM": srcdom,
            "SRCVAR": srcvar,
            "SRCSEQ": srcseq if srcseq is not None else "",
        })

    columns = [
        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "STARTDT", "ADT", "AVAL",
        "CNSR", "EVNTDESC", "CNSDTDSC", "SRCDOM", "SRCVAR", "SRCSEQ",
    ]
    with open(f"{OUTPUT_DIR}/adtte.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
