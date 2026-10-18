# Reference solution for the yamaa benchmark adam-adlb-supplb-padded-key (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def main():
    with open(f"{INPUT_DIR}/lb.csv", newline="") as f:
        lb_rows = list(csv.DictReader(f))
    with open(f"{INPUT_DIR}/supplb.csv", newline="") as f:
        supp_rows = list(csv.DictReader(f))

    # Index SUPPLB by (STUDYID, USUBJID, IDVARVAL) exactly as collected:
    # IDVARVAL must equal the LBSEQ padded with spaces to eight characters.
    supp = {}
    for row in supp_rows:
        key = (row["STUDYID"], row["USUBJID"], row["IDVARVAL"])
        supp[key] = row["QVAL"]

    out_rows = []
    for row in lb_rows:
        padded = row["LBSEQ"].rjust(8)
        key = (row["STUDYID"], row["USUBJID"], padded)
        qval = supp.get(key, "")
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "LBSEQ": row["LBSEQ"],
                "QVAL": qval,
                "QVAL_NAMED": qval,
            }
        )

    with open(f"{OUTPUT_DIR}/adlb.csv", "w", newline="") as f:
        writer = csv.DictWriter(
            f, fieldnames=["STUDYID", "USUBJID", "LBSEQ", "QVAL", "QVAL_NAMED"]
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
