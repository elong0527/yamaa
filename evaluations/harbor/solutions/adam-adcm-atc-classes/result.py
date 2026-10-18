# Reference solution for the yamaa benchmark adam-adcm-atc-classes (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADCM with ATC class codes and names from FACM and ATC_DICT."""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adcm.csv"

KEYS = ["STUDYID", "USUBJID", "CMSEQ"]
LEVELS = ["ATC1", "ATC2", "ATC3", "ATC4"]


def main() -> None:
    cm = pd.read_csv(f"{INPUT_DIR}/cm.csv", dtype=str, keep_default_na=False)
    facm = pd.read_csv(f"{INPUT_DIR}/facm.csv", dtype=str, keep_default_na=False)
    atc_dict = pd.read_csv(
        f"{INPUT_DIR}/atc_dict.csv", dtype=str, keep_default_na=False
    )

    # Code for each level: FAORRES on the medication's FACM rows whose
    # FATESTCD is that level, matched by STUDYID, USUBJID, and CMSEQ.
    codes = {}
    relevant = facm[facm["FATESTCD"].isin(LEVELS)]
    for _, row in relevant.iterrows():
        key = (row["STUDYID"], row["USUBJID"], row["CMSEQ"])
        codes.setdefault(key, {})[row["FATESTCD"]] = row["FAORRES"]

    # Class name for each code: ATCNAME in the ATC_DICT row with that code.
    names = dict(zip(atc_dict["ATCCODE"], atc_dict["ATCNAME"]))

    records = []
    for _, med in cm.iterrows():
        key = (med["STUDYID"], med["USUBJID"], med["CMSEQ"])
        med_codes = codes.get(key, {})
        record = {col: med[col] for col in KEYS + ["CMTRT", "CMDECOD"]}
        for level in LEVELS:
            code = med_codes.get(level, "")
            record[f"{level}CD"] = code
            record[level] = names.get(code, "") if code else ""
        records.append(record)

    columns = KEYS + ["CMTRT", "CMDECOD"] + LEVELS + [f"{lv}CD" for lv in LEVELS]
    adcm = pd.DataFrame(records, columns=columns)
    adcm.to_csv(OUTPUT_PATH, index=False)


if __name__ == "__main__":
    main()
