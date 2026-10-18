# Reference solution for the yamaa benchmark sdtm-ae-coding (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the SDTM AE dataset by coding reported terms against MedDRA 26.1.

Reads AE_RAW and MEDDRA_26_1 from /app/input and writes ae.csv to
/app/output with one record per adverse event. AETERM is matched exactly
(including letter case) to the dictionary's lowest-level term; the
preferred term and body system come from the matched entry. A term with
no exact match, or a blank term, is coded "NOT CODED" in both fields.
"""

import csv
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

NOT_CODED = "NOT CODED"


def read_rows(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    dictionary = {}
    for entry in read_rows(INPUT / "meddra_26_1.csv"):
        dictionary[entry["LLTNAME"]] = (entry["PTNAME"], entry["SOCNAME"])

    records = []
    for event in read_rows(INPUT / "ae_raw.csv"):
        term = event["AETERM"]
        if term and term in dictionary:
            decoded, body_system = dictionary[term]
        else:
            decoded, body_system = NOT_CODED, NOT_CODED
        records.append(
            {
                "DOMAIN": "AE",
                "STUDYID": event["STUDYID"],
                "USUBJID": event["USUBJID"],
                "AESEQ": event["AESEQ"],
                "AETERM": term,
                "AEDECOD": decoded,
                "AEBODSYS": body_system,
            }
        )

    OUTPUT.mkdir(parents=True, exist_ok=True)
    columns = ["DOMAIN", "STUDYID", "USUBJID", "AESEQ", "AETERM", "AEDECOD", "AEBODSYS"]
    with open(OUTPUT / "ae.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
