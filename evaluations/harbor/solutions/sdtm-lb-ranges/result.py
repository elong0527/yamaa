# Reference solution for the yamaa benchmark sdtm-lb-ranges (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from DM, LBRANGE, and LB_RAW.

One record per collected test result in LB_RAW. The record's test code
and the subject's sex (from DM) together select the reference entry in
LBRANGE, which supplies the standard unit and the lower/upper reference
limits. LBNRIND flags the result against those limits.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/lb.csv"


def read_csv(name):
    return pd.read_csv(
        f"{INPUT_DIR}/{name}", dtype=str, keep_default_na=False
    )


def main():
    dm = read_csv("dm.csv")
    lbrange = read_csv("lbrange.csv")
    lb_raw = read_csv("lb_raw.csv")

    sex_by_subject = {
        (row.STUDYID, row.USUBJID): row.SEX for row in dm.itertuples()
    }
    range_by_test_sex = {
        (row.LBTESTCD, row.SEX): (row.UNIT, row.NRLO, row.NRHI)
        for row in lbrange.itertuples()
    }

    records = []
    for row in lb_raw.itertuples():
        sex = sex_by_subject.get((row.STUDYID, row.USUBJID), "")
        unit, nrlo, nrhi = range_by_test_sex.get((row.LBTESTCD, sex), ("", "", ""))

        result = row.LBSTRESN
        nrind = ""
        if result != "" and nrlo != "" and nrhi != "":
            value = float(result)
            if value < float(nrlo):
                nrind = "LOW"
            elif value > float(nrhi):
                nrind = "HIGH"
            else:
                nrind = "NORMAL"

        records.append(
            {
                "DOMAIN": "LB",
                "STUDYID": row.STUDYID,
                "USUBJID": row.USUBJID,
                "LBSEQ": row.LBSEQ,
                "LBTESTCD": row.LBTESTCD,
                "LBSTRESN": result,
                "LBSTRESU": unit,
                "LBSTNRLO": nrlo,
                "LBSTNRHI": nrhi,
                "LBNRIND": nrind,
            }
        )

    columns = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "LBSTRESN",
        "LBSTRESU",
        "LBSTNRLO",
        "LBSTNRHI",
        "LBNRIND",
    ]
    lb = pd.DataFrame(records, columns=columns)
    lb.to_csv(OUTPUT_PATH, index=False)


if __name__ == "__main__":
    main()
