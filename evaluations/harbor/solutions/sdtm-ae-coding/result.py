# Reference solution for the yamaa benchmark sdtm-ae-coding (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the SDTM AE dataset by coding reported terms against MedDRA 26.1.

Reads AE_RAW and MEDDRA_26_1 from /app/input and writes /app/output/ae.csv
with one record per adverse event. AEDECOD is the preferred term of the
dictionary entry whose lowest-level term equals AETERM exactly (including
letter case); AEBODSYS is that entry's body system. A reported term with
no exact match, or a blank AETERM, gives "NOT CODED" in both columns.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

NOT_CODED = "NOT CODED"


def main() -> None:
    ae_raw = pd.read_csv(
        f"{INPUT_DIR}/ae_raw.csv", dtype=str, keep_default_na=False
    )
    meddra = pd.read_csv(
        f"{INPUT_DIR}/meddra_26_1.csv", dtype=str, keep_default_na=False
    )

    # Exact, case-sensitive lookup on the lowest-level term.
    lookup = {
        row.LLTNAME: (row.PTNAME, row.SOCNAME)
        for row in meddra.itertuples(index=False)
    }

    records = []
    for row in ae_raw.itertuples(index=False):
        term = row.AETERM
        if term and term in lookup:
            ptname, socname = lookup[term]
        else:
            ptname, socname = NOT_CODED, NOT_CODED
        records.append(
            {
                "DOMAIN": "AE",
                "STUDYID": row.STUDYID,
                "USUBJID": row.USUBJID,
                "AESEQ": row.AESEQ,
                "AETERM": term,
                "AEDECOD": ptname,
                "AEBODSYS": socname,
            }
        )

    columns = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "AEDECOD",
        "AEBODSYS",
    ]
    ae = pd.DataFrame(records, columns=columns)
    ae.to_csv(f"{OUTPUT_DIR}/ae.csv", index=False)


if __name__ == "__main__":
    main()
