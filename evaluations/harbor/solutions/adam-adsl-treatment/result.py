# Reference solution for the yamaa benchmark adam-adsl-treatment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADSL (one record per subject) from DM and EX.

Reads /app/input/dm.csv and /app/input/ex.csv and writes
/app/output/adsl.csv with columns:
STUDYID, USUBJID, TRT01A, TRTSDT, TRTSDTM, TRTSTMF, TRTEDT, TRTEDTM,
TRTETMF, TRTDURD, SAFFL
"""

import csv
from datetime import datetime, time

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adsl.csv"

NAMED_TREATMENTS = {"VITAMIN D3", "PLACEBO"}
COLUMNS = [
    "STUDYID",
    "USUBJID",
    "TRT01A",
    "TRTSDT",
    "TRTSDTM",
    "TRTSTMF",
    "TRTEDT",
    "TRTEDTM",
    "TRTETMF",
    "TRTDURD",
    "SAFFL",
]


def parse_dtc(value):
    """Split an ISO 8601 date/time collection value.

    Returns (date, time_or_None, has_value). A value without a "T"
    separator was collected as a date only.
    """
    value = (value or "").strip()
    if not value:
        return None, None, False
    if "T" in value:
        date_part, time_part = value.split("T", 1)
        parsed_time = datetime.strptime(time_part.strip(), "%H:%M:%S").time()
        return (
            datetime.strptime(date_part.strip(), "%Y-%m-%d").date(),
            parsed_time,
            True,
        )
    return datetime.strptime(value, "%Y-%m-%d").date(), None, True


def read_rows(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def main():
    dm_rows = read_rows(f"{INPUT_DIR}/dm.csv")
    ex_rows = read_rows(f"{INPUT_DIR}/ex.csv")

    ex_by_subject = {}
    for row in ex_rows:
        ex_by_subject.setdefault(row["USUBJID"], []).append(row)

    output_rows = []
    for dm in dm_rows:
        subject_ex = ex_by_subject.get(dm["USUBJID"], [])

        # Qualifying exposure records: named treatment and a start value.
        qualifying = []
        for row in subject_ex:
            if row["EXTRT"].strip() not in NAMED_TREATMENTS:
                continue
            start_date, start_time, has_start = parse_dtc(row["EXSTDTC"])
            if has_start:
                effective = datetime.combine(
                    start_date, start_time or time(0, 0, 0)
                )
                qualifying.append(
                    (effective, int(row["EXSEQ"]), row, start_date, start_time)
                )
        qualifying.sort(key=lambda item: (item[0], item[1]))

        trtsdt = trtsdtm = trtstmf = ""
        trtedt = trtedtm = trtetmf = ""
        trtdurd = ""

        if qualifying:
            _, _, first_row, start_date, start_time = qualifying[0]
            trt01a = first_row["EXTRT"].strip().upper()
            trtsdt = start_date.isoformat()
            trtsdtm = (
                f"{start_date.isoformat()}T"
                f"{(start_time or time(0, 0, 0)).isoformat()}"
            )
            trtstmf = "H" if start_time is None else ""
        else:
            actarm = (dm.get("ACTARM") or "").strip()
            trt01a = actarm.upper() if actarm else "NOT TREATED"

        # Latest end among records naming a treatment, with or without a
        # start; an end collected without a time counts as 23:59:59.
        end_candidates = []
        for row in subject_ex:
            if row["EXTRT"].strip() not in NAMED_TREATMENTS:
                continue
            end_date, end_time, has_end = parse_dtc(row["EXENDTC"])
            if has_end:
                effective = datetime.combine(
                    end_date, end_time or time(23, 59, 59)
                )
                end_candidates.append((effective, end_date, end_time))

        if end_candidates:
            _, end_date, end_time = max(
                end_candidates, key=lambda item: item[0]
            )
            trtedt = end_date.isoformat()
            trtedtm = (
                f"{end_date.isoformat()}T"
                f"{(end_time or time(23, 59, 59)).isoformat()}"
            )
            trtetmf = "H" if end_time is None else ""

        if trtsdt and trtedt:
            start = datetime.strptime(trtsdt, "%Y-%m-%d").date()
            end = datetime.strptime(trtedt, "%Y-%m-%d").date()
            trtdurd = str((end - start).days + 1)

        output_rows.append(
            {
                "STUDYID": dm["STUDYID"],
                "USUBJID": dm["USUBJID"],
                "TRT01A": trt01a,
                "TRTSDT": trtsdt,
                "TRTSDTM": trtsdtm,
                "TRTSTMF": trtstmf,
                "TRTEDT": trtedt,
                "TRTEDTM": trtedtm,
                "TRTETMF": trtetmf,
                "TRTDURD": trtdurd,
                "SAFFL": "Y" if trtsdt else "N",
            }
        )

    with open(OUTPUT_PATH, "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(output_rows)


if __name__ == "__main__":
    main()
