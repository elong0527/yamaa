# Reference solution for the yamaa benchmark adam-adae-query-flags (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADAE with query grouping columns (SMQ01, SMQ02, CQ01).

Reads AE and QUERIES from /app/input and writes /app/output/adae.csv.
Membership of an event in a grouping is decided by its AEDECOD: the
QUERIES dictionary lists coded terms under query groupings, identified
by PREFIX (SMQ01 = first standardized query, SMQ02 = second standardized
query, CQ01 = sponsor-defined customized query).
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adae.csv"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AETERM",
    "AEDECOD",
    "SMQ01NAM",
    "SMQ01CD",
    "SMQ01SC",
    "SMQ02NAM",
    "SMQ02CD",
    "SMQ02SC",
    "CQ01NAM",
]


def main() -> None:
    ae = pd.read_csv(
        f"{INPUT_DIR}/ae.csv", dtype=str, keep_default_na=False
    )
    queries = pd.read_csv(
        f"{INPUT_DIR}/queries.csv", dtype=str, keep_default_na=False
    )

    # Index the dictionary by (prefix, coded term).
    lookup = {}
    for row in queries.itertuples(index=False):
        lookup[(row.PREFIX, row.TERM)] = (row.GRPNAME, row.GRPID, row.SCOPE)

    def grouping(decod: str, prefix: str):
        if decod == "":
            return ("", "", "")
        return lookup.get((prefix, decod), ("", "", ""))

    records = []
    for row in ae.itertuples(index=False):
        smq01 = grouping(row.AEDECOD, "SMQ01")
        smq02 = grouping(row.AEDECOD, "SMQ02")
        cq01 = grouping(row.AEDECOD, "CQ01")
        records.append(
            {
                "STUDYID": row.STUDYID,
                "USUBJID": row.USUBJID,
                "AESEQ": row.AESEQ,
                "AETERM": row.AETERM,
                "AEDECOD": row.AEDECOD,
                "SMQ01NAM": smq01[0],
                "SMQ01CD": smq01[1],
                "SMQ01SC": smq01[2],
                "SMQ02NAM": smq02[0],
                "SMQ02CD": smq02[1],
                "SMQ02SC": smq02[2],
                "CQ01NAM": cq01[0],
            }
        )

    adae = pd.DataFrame.from_records(records, columns=COLUMNS)
    adae.to_csv(OUTPUT_PATH, index=False)


if __name__ == "__main__":
    main()
