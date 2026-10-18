# Reference solution for the yamaa benchmark adam-adcm-on-treatment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADCM (one record per concomitant medication) from ADSL and CM.

ONTRTFL is "Y" when the medication's dates overlap the subject's treatment
period [TRTSDT, TRTEDT]; boundary contact counts as overlap. A missing
medication start or end date is treated as overlapping unless the known
dates rule overlap out. A missing TRTEDT leaves the period open-ended.
A subject with no TRTSDT is never flagged.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def to_date(series):
    return pd.to_datetime(series, errors="coerce")


def fmt_date(series):
    return series.dt.strftime("%Y-%m-%d").fillna("")


def main():
    adsl = pd.read_csv(f"{INPUT_DIR}/adsl.csv", dtype=str, keep_default_na=False)
    cm = pd.read_csv(f"{INPUT_DIR}/cm.csv", dtype=str, keep_default_na=False)

    adsl["TRTSDT_D"] = to_date(adsl["TRTSDT"])
    adsl["TRTEDT_D"] = to_date(adsl["TRTEDT"])

    trt = adsl.set_index(["STUDYID", "USUBJID"])[["TRTSDT_D", "TRTEDT_D"]]
    cm = cm.join(trt, on=["STUDYID", "USUBJID"])

    cm["ASTDT_D"] = to_date(cm["CMSTDTC"])
    cm["AENDT_D"] = to_date(cm["CMENDTC"])

    def ontrtfl(row):
        tsdt, tedt = row["TRTSDT_D"], row["TRTEDT_D"]
        if pd.isna(tsdt):
            return ""
        astdt, aendt = row["ASTDT_D"], row["AENDT_D"]
        # Known dates rule overlap out only in these two cases.
        if pd.notna(aendt) and aendt < tsdt:
            return ""
        if pd.notna(astdt) and pd.notna(tedt) and astdt > tedt:
            return ""
        return "Y"

    cm["ONTRTFL"] = cm.apply(ontrtfl, axis=1)

    out = pd.DataFrame(
        {
            "STUDYID": cm["STUDYID"],
            "USUBJID": cm["USUBJID"],
            "CMSEQ": cm["CMSEQ"],
            "CMTRT": cm["CMTRT"],
            "ASTDT": fmt_date(cm["ASTDT_D"]),
            "AENDT": fmt_date(cm["AENDT_D"]),
            "TRTSDT": fmt_date(cm["TRTSDT_D"]),
            "TRTEDT": fmt_date(cm["TRTEDT_D"]),
            "ONTRTFL": cm["ONTRTFL"],
        }
    )
    out.to_csv(f"{OUTPUT_DIR}/adcm.csv", index=False)


if __name__ == "__main__":
    main()
