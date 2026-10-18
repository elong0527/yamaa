# Reference solution for the yamaa benchmark adam-adtte-os (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADTTE overall-survival dataset from ADRS and ADSL.

One record per subject. A qualifying death is an ADRS record with
PARAMCD "DEATH", AVALC "Y", ANL01FL "Y" and a recorded ADT; the
earliest such date (lowest ASEQ on a tie) is the event. Subjects
without a qualifying death are censored at their last-alive date when
it falls after randomization, and at randomization otherwise.
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


def read_rows(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = read_rows(f"{INPUT_DIR}/adsl.csv")
    adrs = read_rows(f"{INPUT_DIR}/adrs.csv")

    # Select each subject's qualifying death record: earliest ADT,
    # lowest ASEQ on a tied date.
    deaths = {}
    for row in adrs:
        if row["PARAMCD"] != "DEATH":
            continue
        if row["AVALC"] != "Y" or row["ANL01FL"] != "Y":
            continue
        death_date = parse_date(row["ADT"])
        if death_date is None:
            continue
        key = (death_date, int(row["ASEQ"]))
        subject = row["USUBJID"]
        if subject not in deaths or key < deaths[subject][0]:
            deaths[subject] = (key, row)

    records = []
    for subject_row in adsl:
        studyid = subject_row["STUDYID"]
        usubjid = subject_row["USUBJID"]
        startdt = parse_date(subject_row["RANDDT"])
        lstalvdt = parse_date(subject_row["LSTALVDT"])

        if usubjid in deaths:
            _, death_row = deaths[usubjid]
            death_date = parse_date(death_row["ADT"])
            adt = death_date if death_date >= startdt else startdt
            cnsr = "0"
            evntdesc = "Death"
            cnsdtdsc = ""
            srcdom = "ADRS"
            srcvar = "ADT"
            srcseq = death_row["ASEQ"]
        else:
            at_last_alive = lstalvdt is not None and lstalvdt > startdt
            adt = lstalvdt if at_last_alive else startdt
            cnsr = "1"
            if at_last_alive:
                evntdesc = "Alive"
                cnsdtdsc = "Alive During Study"
                srcvar = "LSTALVDT"
            else:
                evntdesc = "Randomization"
                cnsdtdsc = "Randomization"
                srcvar = "RANDDT"
            srcdom = "ADSL"
            srcseq = ""

        aval = (adt - startdt).days + 1
        records.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "PARAMCD": "OS",
                "PARAM": "Overall Survival",
                "STARTDT": startdt.isoformat(),
                "ADT": adt.isoformat(),
                "AVAL": str(aval),
                "CNSR": cnsr,
                "EVNTDESC": evntdesc,
                "CNSDTDSC": cnsdtdsc,
                "SRCDOM": srcdom,
                "SRCVAR": srcvar,
                "SRCSEQ": srcseq,
            }
        )

    columns = [
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
    with open(f"{OUTPUT_DIR}/adtte.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
