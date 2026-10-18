# Reference solution for the yamaa benchmark adam-adae-query-flags (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADAE dataset with query grouping columns from AE and QUERIES.

Membership of an adverse event in a query grouping is decided by its
AEDECOD: the QUERIES dictionary lists coded terms under groupings, and
every event with the same coded term shows the same grouping entries.
SMQ01/SMQ02 columns carry the name, code and scope of the first/second
standardized query grouping for the term; CQ01NAM carries the name of
the sponsor-defined customized query grouping. Events without an
AEDECOD, or whose term is not listed under a grouping, get no value in
that grouping's columns.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    ae_rows = read_csv(os.path.join(INPUT_DIR, "ae.csv"))
    query_rows = read_csv(os.path.join(INPUT_DIR, "queries.csv"))

    # Index the dictionary by prefix and coded term. A term appears at
    # most once under a given prefix; each entry gives the grouping
    # name, its code (standardized groupings only) and the scope of the
    # term's membership (standardized groupings only).
    dictionary = {}
    for row in query_rows:
        dictionary[(row["PREFIX"], row["TERM"])] = row

    def grouping(prefix, term):
        """Return (name, code, scope) for a term under a prefix."""
        entry = dictionary.get((prefix, term))
        if entry is None:
            return "", "", ""
        return entry["GRPNAME"], entry["GRPID"], entry["SCOPE"]

    out_rows = []
    for ae in ae_rows:
        decod = ae["AEDECOD"]
        if decod:
            smq01nam, smq01cd, smq01sc = grouping("SMQ01", decod)
            smq02nam, smq02cd, smq02sc = grouping("SMQ02", decod)
            cq01nam, _, _ = grouping("CQ01", decod)
        else:
            smq01nam = smq01cd = smq01sc = ""
            smq02nam = smq02cd = smq02sc = ""
            cq01nam = ""
        out_rows.append(
            {
                "STUDYID": ae["STUDYID"],
                "USUBJID": ae["USUBJID"],
                "AESEQ": ae["AESEQ"],
                "AETERM": ae["AETERM"],
                "AEDECOD": decod,
                "SMQ01NAM": smq01nam,
                "SMQ01CD": smq01cd,
                "SMQ01SC": smq01sc,
                "SMQ02NAM": smq02nam,
                "SMQ02CD": smq02cd,
                "SMQ02SC": smq02sc,
                "CQ01NAM": cq01nam,
            }
        )

    columns = [
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
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(
        os.path.join(OUTPUT_DIR, "adae.csv"), "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
