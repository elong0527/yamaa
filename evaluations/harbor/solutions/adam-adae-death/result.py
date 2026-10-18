# Reference solution for the yamaa benchmark adam-adae-death (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADAE (one record per adverse event) with death derivations.

Reads AE and DM from /app/input and writes /app/output/adae.csv.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adae.csv"


def main() -> None:
    ae = pd.read_csv(f"{INPUT_DIR}/ae.csv", dtype=str, keep_default_na=False)
    dm = pd.read_csv(f"{INPUT_DIR}/dm.csv", dtype=str, keep_default_na=False)

    ae["AESEQ"] = ae["AESEQ"].astype(int)

    # Death date recorded in DM, per subject (blank when none recorded).
    dm_death = {}
    for _, row in dm.iterrows():
        dthdt = row["DTHDT"].strip()
        if dthdt:
            dm_death[row["USUBJID"]] = dthdt

    # Each subject's chosen fatal event: latest ASTDT, tie broken by
    # highest AESEQ. Supplies DTHCAUS and DTHDT.
    fatal = ae[ae["AEOUT"] == "FATAL"].copy()
    fatal = fatal.sort_values(["USUBJID", "ASTDT", "AESEQ"])
    chosen_fatal = {}
    for _, row in fatal.iterrows():
        # Ascending sort: later rows overwrite, leaving the latest
        # start date (and highest AESEQ on a tie) as the chosen event.
        chosen_fatal[row["USUBJID"]] = row

    dthfl_values = []
    dthcaus_values = []
    dthdt_values = []
    for _, row in ae.iterrows():
        subj = row["USUBJID"]
        fatal_row = chosen_fatal.get(subj)
        dm_dthdt = dm_death.get(subj)

        if fatal_row is not None or dm_dthdt is not None:
            dthfl_values.append("Y")
        else:
            dthfl_values.append("")

        if fatal_row is not None:
            dthcaus_values.append(fatal_row["AEDECOD"])
            dthdt_values.append(fatal_row["ASTDT"])
        else:
            dthcaus_values.append("")
            dthdt_values.append(dm_dthdt if dm_dthdt is not None else "")

    ae["DTHFL"] = dthfl_values
    ae["DTHCAUS"] = dthcaus_values
    ae["DTHDT"] = dthdt_values

    out = ae[
        ["STUDYID", "USUBJID", "AESEQ", "AEDECOD", "ASTDT", "DTHFL", "DTHCAUS", "DTHDT"]
    ]
    out.to_csv(OUTPUT_PATH, index=False)


if __name__ == "__main__":
    main()
