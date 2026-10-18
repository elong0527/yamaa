# Reference solution for the yamaa benchmark adam-adae-onset-emergence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADAE dataset (one record per adverse event) from ADSL and AE.

Treatment emergence is decided at second precision against the window from
first exposure (TRTSDTM) through 30 days after last exposure (TEENDDTM).
"""

import os
from datetime import timedelta

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def to_datetime(series):
    """Parse ISO datetime text; empty text and unparseable values are NaT.

    A value collected without seconds (e.g. '2025-03-04T11:45') parses with
    seconds taken as zero.
    """
    return pd.to_datetime(series.replace("", None), errors="coerce", format="mixed")


def fmt_datetime(value):
    if pd.isna(value):
        return ""
    return value.strftime("%Y-%m-%dT%H:%M:%S")


def main():
    adsl = pd.read_csv(
        os.path.join(INPUT_DIR, "adsl.csv"), dtype=str, keep_default_na=False
    )
    ae = pd.read_csv(
        os.path.join(INPUT_DIR, "ae.csv"), dtype=str, keep_default_na=False
    )

    adsl["TRTSDTM_DT"] = to_datetime(adsl["TRTSDTM"])
    adsl["TRTENDTM_DT"] = to_datetime(adsl["TRTENDTM"])
    adsl["TEENDDTM_DT"] = to_datetime(adsl["TEENDDTM"])
    # TEENDDTM is 30 days after last exposure when not supplied directly.
    missing_end = adsl["TEENDDTM_DT"].isna() & adsl["TRTENDTM_DT"].notna()
    adsl.loc[missing_end, "TEENDDTM_DT"] = adsl.loc[
        missing_end, "TRTENDTM_DT"
    ] + timedelta(days=30)

    df = ae.merge(
        adsl[["STUDYID", "USUBJID", "TRTSDTM_DT", "TEENDDTM_DT"]],
        on=["STUDYID", "USUBJID"],
        how="left",
    )
    df["ASTDTM_DT"] = to_datetime(df["AESTDTC"])

    def trtemfl(row):
        if pd.isna(row["TRTSDTM_DT"]):
            return ""
        if pd.isna(row["ASTDTM_DT"]):
            return "Y"
        if row["ASTDTM_DT"] < row["TRTSDTM_DT"]:
            return ""
        if pd.notna(row["TEENDDTM_DT"]) and row["ASTDTM_DT"] > row["TEENDDTM_DT"]:
            return ""
        return "Y"

    df["TRTEMFL"] = df.apply(trtemfl, axis=1)

    # AOCCFL flags each subject's earliest treatment-emergent event by onset
    # moment; ties on the same onset second go to the lower AESEQ. A
    # treatment-emergent event with no onset moment is never flagged.
    df["AOCCFL"] = ""
    df["AESEQ_NUM"] = pd.to_numeric(df["AESEQ"], errors="coerce")
    emergent = df[(df["TRTEMFL"] == "Y") & df["ASTDTM_DT"].notna()].sort_values(
        ["STUDYID", "USUBJID", "ASTDTM_DT", "AESEQ_NUM"]
    )
    first_idx = emergent.groupby(["STUDYID", "USUBJID"]).head(1).index
    df.loc[first_idx, "AOCCFL"] = "Y"

    out = pd.DataFrame(
        {
            "STUDYID": df["STUDYID"],
            "USUBJID": df["USUBJID"],
            "AESEQ": df["AESEQ"],
            "AETERM": df["AETERM"],
            "ASTDTM": df["ASTDTM_DT"].map(fmt_datetime),
            "TRTSDTM": df["TRTSDTM_DT"].map(fmt_datetime),
            "TEENDDTM": df["TEENDDTM_DT"].map(fmt_datetime),
            "TRTEMFL": df["TRTEMFL"],
            "AOCCFL": df["AOCCFL"],
        }
    )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    out.to_csv(os.path.join(OUTPUT_DIR, "adae.csv"), index=False)


if __name__ == "__main__":
    main()
