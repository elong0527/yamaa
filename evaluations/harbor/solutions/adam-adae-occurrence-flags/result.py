# Reference solution for the yamaa benchmark adam-adae-occurrence-flags (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADAE occurrence flags (AOCCFL, AOCCSFL, AOCCPFL) from ADAE_RAW.

An event is treatment-emergent when TRTEMFL is "Y"; only such events can
be flagged. Within each grouping, the first treatment-emergent event is
the one with the earliest ASTDT, with the lower AESEQ breaking a tie.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AEBODSYS",
    "AEDECOD",
    "ASTDT",
    "TRTEMFL",
    "AOCCFL",
    "AOCCSFL",
    "AOCCPFL",
]


def first_event_index(df, group_cols):
    """Index of the first treatment-emergent event within each group."""
    te = df[df["TRTEMFL"] == "Y"].copy()
    te["_seq"] = te["AESEQ"].astype(int)
    te = te.sort_values(["ASTDT", "_seq"], kind="mergesort")
    return set(te.groupby(group_cols, sort=False).head(1).index)


def main():
    df = pd.read_csv(
        f"{INPUT_DIR}/adae_raw.csv", dtype=str, keep_default_na=False
    )

    first_subject = first_event_index(df, ["STUDYID", "USUBJID"])
    first_bodsys = first_event_index(df, ["STUDYID", "USUBJID", "AEBODSYS"])
    first_pt = first_event_index(
        df, ["STUDYID", "USUBJID", "AEBODSYS", "AEDECOD"]
    )

    df["AOCCFL"] = ["Y" if i in first_subject else "" for i in df.index]
    df["AOCCSFL"] = ["Y" if i in first_bodsys else "" for i in df.index]
    df["AOCCPFL"] = ["Y" if i in first_pt else "" for i in df.index]

    out = df[COLUMNS]
    out.to_csv(f"{OUTPUT_DIR}/adae.csv", index=False)


if __name__ == "__main__":
    main()
