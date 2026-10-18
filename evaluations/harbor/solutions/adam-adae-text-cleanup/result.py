# Reference solution for the yamaa benchmark adam-adae-text-cleanup (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import re

import pandas as pd

ae = pd.read_csv("/app/input/ae.csv", dtype=str, keep_default_na=False)

pattern = re.compile(r"^AE-(\d{3})$")


def aerefnum(aespid):
    if aespid == "":
        return 0
    match = pattern.match(aespid)
    if match:
        return int(match.group(1))
    return -1


ae["AEREFNUM"] = ae["AESPID"].apply(aerefnum)
ae["AETERMLO"] = ae["AETERM"].str.lower()
ae["AERELLC"] = ae["AEREL"].apply(lambda value: value.lower() if value != "" else "not reported")
ae["AREL"] = ae["AERELLC"].str.upper()

columns = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AESPID",
    "AEREFNUM",
    "AETERM",
    "AETERMLO",
    "AERELLC",
    "AREL",
]

ae[columns].to_csv("/app/output/adae.csv", index=False)
