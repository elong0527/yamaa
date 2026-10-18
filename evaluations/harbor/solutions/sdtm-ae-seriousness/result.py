# Reference solution for the yamaa benchmark sdtm-ae-seriousness (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM AE dataset from AE_RAW.

One record per adverse event. AESER is "Y" when any of the seriousness
criteria (AESDTH, AESLIFE, AESHOSP, AESDISAB, AESCONG, AESMIE) is "Y",
and "N" when no criterion is "Y" (including when every criterion is
empty). Criterion flags keep their collected values; an empty flag
stays empty rather than becoming "N".
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

CRITERIA = ["AESDTH", "AESLIFE", "AESHOSP", "AESDISAB", "AESCONG", "AESMIE"]
COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AETERM",
    "AESER",
] + CRITERIA


def main() -> None:
    ae_raw = pd.read_csv(
        f"{INPUT_DIR}/ae_raw.csv", dtype=str, keep_default_na=False
    )

    ae = pd.DataFrame()
    ae["STUDYID"] = ae_raw["STUDYID"]
    ae["USUBJID"] = ae_raw["USUBJID"]
    ae["AESEQ"] = ae_raw["AESEQ"]
    ae["AETERM"] = ae_raw["AETERM"]
    ae["DOMAIN"] = "AE"
    ae["AESER"] = [
        "Y" if any(row[col] == "Y" for col in CRITERIA) else "N"
        for _, row in ae_raw.iterrows()
    ]
    for col in CRITERIA:
        ae[col] = ae_raw[col]

    ae = ae[COLUMNS]
    ae.to_csv(f"{OUTPUT_DIR}/ae.csv", index=False)


if __name__ == "__main__":
    main()
