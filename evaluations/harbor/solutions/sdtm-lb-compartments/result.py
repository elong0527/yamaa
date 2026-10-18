# Reference solution for the yamaa benchmark sdtm-lb-compartments (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset (Interleukin 13, skin compartments) from BX_RAW.

One record per skin compartment per subject: every subject gets a
non-lesional record, and a subject also gets a lesional record when a
cohort is recorded for the subject and the cohort is not "NONAD".
Within a subject the lesional record is sequenced before the
non-lesional record.
"""

import csv

INPUT_CSV = "/app/input/bx_raw.csv"
OUTPUT_CSV = "/app/output/lb.csv"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "LBSEQ",
    "LBTESTCD",
    "LBTEST",
    "LBSPEC",
    "LBLOC",
    "LBORRES",
    "LBORRESU",
    "LBSTRESN",
    "LBSTAT",
]


def make_record(subject, seq, location, result, unit):
    """Assemble one LB record for a single compartment."""
    result = (result or "").strip()
    if result:
        numeric = float(result)
        status = ""
    else:
        numeric = ""
        status = "NOT DONE"
    return {
        "DOMAIN": "LB",
        "STUDYID": subject["STUDYID"],
        "USUBJID": subject["USUBJID"],
        "LBSEQ": seq,
        "LBTESTCD": "IL13",
        "LBTEST": "Interleukin 13",
        "LBSPEC": "SKIN",
        "LBLOC": location,
        "LBORRES": result,
        "LBORRESU": unit,
        "LBSTRESN": numeric,
        "LBSTAT": status,
    }


def main():
    with open(INPUT_CSV, newline="") as handle:
        subjects = list(csv.DictReader(handle))

    records = []
    for subject in subjects:
        unit = (subject.get("RESU") or "").strip()
        cohort = (subject.get("COHORT") or "").strip()
        seq = 0
        # Lesional record first, when a cohort is recorded and is not NONAD.
        if cohort and cohort != "NONAD":
            seq += 1
            records.append(
                make_record(subject, seq, "LESIONAL", subject.get("LESRES"), unit)
            )
        # Every subject has a non-lesional record.
        seq += 1
        records.append(
            make_record(subject, seq, "NON-LESIONAL", subject.get("NLESRES"), unit)
        )

    with open(OUTPUT_CSV, "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
