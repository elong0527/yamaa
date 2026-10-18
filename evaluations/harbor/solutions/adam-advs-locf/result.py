# Reference solution for the yamaa benchmark adam-advs-locf (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADVS dataset from the VS dataset with LOCF imputation.

One record per subject per parameter per analysis visit. AVAL is the
collected value when the record has one; otherwise it is the value from
the closest earlier visit, in visit-number order, that has one, for the
same subject and parameter. A gap before the first collected value has
no value. A collected value of zero is carried like any other value.
"""

import pandas as pd

INPUT_VS = "/app/input/vs.csv"
OUTPUT_ADVS = "/app/output/advs.csv"


def main() -> None:
    vs = pd.read_csv(INPUT_VS, dtype=str, keep_default_na=False)
    vs["AVISITN"] = vs["AVISITN"].astype(int)
    vs = vs.sort_values(
        ["USUBJID", "PARAMCD", "AVISITN"], kind="stable"
    ).reset_index(drop=True)

    last_value = {}
    aval = []
    for row in vs.itertuples(index=False):
        key = (row.USUBJID, row.PARAMCD)
        if row.AVALCOL != "":
            last_value[key] = row.AVALCOL
        aval.append(last_value.get(key, ""))
    vs["AVAL"] = aval

    advs = vs[["USUBJID", "PARAMCD", "AVISITN", "AVAL"]]
    advs.to_csv(OUTPUT_ADVS, index=False)


if __name__ == "__main__":
    main()
