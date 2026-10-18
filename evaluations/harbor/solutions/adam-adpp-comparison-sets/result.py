# Reference solution for the yamaa benchmark adam-adpp-comparison-sets (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADPP from ADPP_RAW, ADSL, and COMPARE.

One record per subject, parameter, and period. ANL01FL / ANL02FL mark
the records used by comparison 1 / comparison 2: a record is used when
its treatment is one of the comparison's two treatments, its own AVAL
is evaluable (present and greater than zero), and the same subject has
an evaluable AVAL for the same parameter under the comparison's other
treatment.
"""

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(name):
    with open(f"{INPUT_DIR}/{name}", newline="") as f:
        return list(csv.DictReader(f))


def evaluable(value):
    if value is None or value == "":
        return False
    return float(value) > 0


def main():
    raw = read_csv("adpp_raw.csv")
    adsl = read_csv("adsl.csv")
    compare = read_csv("compare.csv")

    trtseqp = {row["USUBJID"]: row["TRTSEQP"] for row in adsl}
    comparisons = {
        int(row["COMPNO"]): (row["TESTTRT"], row["REFTRT"]) for row in compare
    }

    # Evaluable (subject, parameter, treatment) combinations present.
    evaluable_keys = set()
    for row in raw:
        if evaluable(row["AVAL"]):
            evaluable_keys.add((row["USUBJID"], row["PARAMCD"], row["TRTA"]))

    def used(row, compno):
        test, ref = comparisons[compno]
        if row["TRTA"] not in (test, ref):
            return False
        if not evaluable(row["AVAL"]):
            return False
        other = ref if row["TRTA"] == test else test
        return (row["USUBJID"], row["PARAMCD"], other) in evaluable_keys

    columns = [
        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "APERIOD", "TRTA",
        "TRTSEQP", "AVAL", "ANL01FL", "ANL02FL",
    ]
    with open(f"{OUTPUT_DIR}/adpp.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        for row in raw:
            writer.writerow({
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": row["PARAMCD"],
                "PARAM": row["PARAM"],
                "APERIOD": row["APERIOD"],
                "TRTA": row["TRTA"],
                "TRTSEQP": trtseqp.get(row["USUBJID"], ""),
                "AVAL": row["AVAL"],
                "ANL01FL": "Y" if used(row, 1) else "",
                "ANL02FL": "Y" if used(row, 2) else "",
            })


if __name__ == "__main__":
    main()
