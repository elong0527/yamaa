# Reference solution for the yamaa benchmark adam-adex-uncollected (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADEX dataset from EX and SUBJECT_TREATMENT.

One record per subject and treatment listed in SUBJECT_TREATMENT.
DOSECUM is the total of the recorded EXDOSE values on the EX records
for the subject and treatment; it has no value when no dose was ever
recorded, while a recorded total of zero is kept as zero. NDOSREC is
the number of EX records for the subject and treatment, with no value
when the treatment has no EX record at all. NDOSVAL is the number of
those EX records that carry a recorded dose; an explicitly recorded
zero counts, and NDOSVAL is 0 when the treatment has EX records but
every dose was left blank.
"""

import csv
from collections import defaultdict

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adex.csv"


def read_rows(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def format_number(value):
    """Write a number without a trailing decimal part when whole."""
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    treatments = read_rows(f"{INPUT_DIR}/subject_treatment.csv")
    exposure = read_rows(f"{INPUT_DIR}/ex.csv")

    doses_by_key = defaultdict(list)
    for row in exposure:
        key = (row["STUDYID"], row["USUBJID"], row["EXTRT"])
        doses_by_key[key].append(row["EXDOSE"])

    records = []
    for row in treatments:
        key = (row["STUDYID"], row["USUBJID"], row["EXTRT"])
        doses = doses_by_key.get(key)
        if doses is None:
            n_records = ""
            n_values = ""
            dose_cum = ""
        else:
            recorded = [float(dose) for dose in doses if dose.strip() != ""]
            n_records = str(len(doses))
            n_values = str(len(recorded))
            dose_cum = format_number(sum(recorded)) if recorded else ""
        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "EXTRT": row["EXTRT"],
                "DOSECUM": dose_cum,
                "NDOSREC": n_records,
                "NDOSVAL": n_values,
            }
        )

    columns = ["STUDYID", "USUBJID", "EXTRT", "DOSECUM", "NDOSREC", "NDOSVAL"]
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
