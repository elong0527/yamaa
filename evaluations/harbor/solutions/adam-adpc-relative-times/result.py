# Reference solution for the yamaa benchmark adam-adpc-relative-times (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from datetime import datetime

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

# The drug each analyte measures: the parent drug DRGX and its
# metabolite MET1 both belong to "DRUG X"; DRGY belongs to "DRUG Y".
ANALYTE_DRUG = {"DRGX": "DRUG X", "MET1": "DRUG X", "DRGY": "DRUG Y"}


def parse_datetime(value):
    """Parse an ISO 8601 date-time as written.

    Returns None when the value carries a date but no time; such a
    time is never filled in.
    """
    if value is None:
        return None
    value = value.strip()
    if "T" not in value:
        return None
    return datetime.fromisoformat(value)


def main():
    with open(f"{INPUT_DIR}/ex.csv", newline="") as f:
        ex_rows = list(csv.DictReader(f))
    with open(f"{INPUT_DIR}/pc.csv", newline="") as f:
        pc_rows = list(csv.DictReader(f))

    # Dose start date-times per subject and treatment, earliest first.
    # Each entry keeps the collected text alongside the parsed value.
    doses = {}
    for row in ex_rows:
        dtm = parse_datetime(row["EXSTDTC"])
        if dtm is None:
            continue
        key = (row["USUBJID"], row["EXTRT"])
        doses.setdefault(key, []).append((dtm, row["EXSTDTC"].strip()))
    for subject_doses in doses.values():
        subject_doses.sort(key=lambda item: item[0])

    out_rows = []
    for row in pc_rows:
        drug = ANALYTE_DRUG[row["PCTESTCD"]]
        subject_doses = doses.get((row["USUBJID"], drug), [])
        adtm = parse_datetime(row["PCDTC"])

        adtm_text = row["PCDTC"].strip() if adtm is not None else ""
        fanldtm = subject_doses[0][0] if subject_doses else None
        fanldtm_text = subject_doses[0][1] if subject_doses else ""

        # Reference dose: the latest dose of the analyte's drug starting
        # at or before the sample; a sample before every dose refers to
        # the first dose. No value when ADTM or FANLDTM has no value.
        pcrftdtm = None
        pcrftdtm_text = ""
        if adtm is not None and fanldtm is not None:
            ref = subject_doses[0]
            for dose in subject_doses:
                if dose[0] <= adtm:
                    ref = dose
                else:
                    break
            pcrftdtm, pcrftdtm_text = ref

        afrlt = ""
        if adtm is not None and fanldtm is not None:
            afrlt = (adtm - fanldtm).total_seconds() / 3600
        arrlt = ""
        if adtm is not None and pcrftdtm is not None:
            arrlt = (adtm - pcrftdtm).total_seconds() / 3600

        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PCSEQ": row["PCSEQ"],
                "PARAMCD": row["PCTESTCD"],
                "ATPT": row["PCTPT"],
                "ADTM": adtm_text,
                "FANLDTM": fanldtm_text,
                "PCRFTDTM": pcrftdtm_text,
                "AFRLT": afrlt,
                "ARRLT": arrlt,
            }
        )

    fieldnames = [
        "STUDYID",
        "USUBJID",
        "PCSEQ",
        "PARAMCD",
        "ATPT",
        "ADTM",
        "FANLDTM",
        "PCRFTDTM",
        "AFRLT",
        "ARRLT",
    ]
    with open(f"{OUTPUT_DIR}/adpc.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
