# Reference solution for the yamaa benchmark adam-advs-locf (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADVS dataset from the VS dataset.

One record per subject per parameter per analysis visit. AVAL is the
collected value when the record has one; otherwise it is the value from
the closest earlier visit, in visit-number order, that has one, for the
same subject and parameter (last observation carried forward). A gap
before the first collected value has no value. A collected value of
zero is carried like any other value.
"""

import csv

INPUT_PATH = "/app/input/vs.csv"
OUTPUT_PATH = "/app/output/advs.csv"


def main() -> None:
    with open(INPUT_PATH, newline="") as f:
        rows = list(csv.DictReader(f))

    # Walk the records in subject / parameter / visit-number order so
    # each record can see the closest earlier collected value of its
    # group, then map the results back to the original record order.
    order = sorted(
        range(len(rows)),
        key=lambda i: (
            rows[i]["USUBJID"],
            rows[i]["PARAMCD"],
            int(rows[i]["AVISITN"]),
        ),
    )

    last_value = {}
    aval = [""] * len(rows)
    for i in order:
        row = rows[i]
        key = (row["USUBJID"], row["PARAMCD"])
        collected = row["AVALCOL"]
        if collected != "":
            last_value[key] = collected
            aval[i] = collected
        else:
            aval[i] = last_value.get(key, "")

    with open(OUTPUT_PATH, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["USUBJID", "PARAMCD", "AVISITN", "AVAL"])
        for i, row in enumerate(rows):
            writer.writerow(
                [row["USUBJID"], row["PARAMCD"], row["AVISITN"], aval[i]]
            )


if __name__ == "__main__":
    main()
