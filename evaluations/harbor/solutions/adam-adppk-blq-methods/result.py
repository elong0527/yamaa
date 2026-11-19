# Reference solution for the yamaa benchmark adam-adppk-blq-methods (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import numpy as np
import pandas as pd

COLUMNS = [
    "STUDYID", "USUBJID", "ASEQ", "ADTM", "EVID", "AMT",
    "ALLOQ", "CENS", "DV", "MDV", "DVM6", "MDVM6",
]


def with_seconds(value):
    text = str(value).strip()
    if "T" not in text:
        return text + "T00:00:00"
    date, time = text.split("T", 1)
    parts = time.split(":")
    while len(parts) < 3:
        parts.append("00")
    return date + "T" + ":".join(parts)


def number(value):
    if pd.isna(value):
        return ""
    value = float(value)
    if value == int(value):
        return str(int(value))
    return repr(value)


ex = pd.read_csv("/app/input/ex.csv", dtype=str, keep_default_na=False)
pc = pd.read_csv("/app/input/pc.csv", dtype=str, keep_default_na=False)

dose = pd.DataFrame({
    "STUDYID": ex["STUDYID"],
    "USUBJID": ex["USUBJID"],
    "ADTM": ex["EXSTDTC"].map(with_seconds),
    "EVID": 1,
    "AMT": pd.to_numeric(ex["EXDOSE"], errors="coerce"),
    "ALLOQ": np.nan,
    "CENS": 0,
    "DV": np.nan,
    "DVM6": np.nan,
    "SRCSEQ": pd.to_numeric(ex["EXSEQ"], errors="coerce"),
})

pc = pc[pc["PCSTAT"].str.strip().str.upper() != "NOT DONE"].copy()
pc["ADTM"] = pc["PCDTC"].map(with_seconds)
pc["SRCSEQ"] = pd.to_numeric(pc["PCSEQ"], errors="coerce")
pc["RESULT"] = pd.to_numeric(pc["PCSTRESN"], errors="coerce")
pc["ALLOQ"] = pd.to_numeric(pc["PCLLOQ"], errors="coerce")
pc["BLQ"] = pc["RESULT"].isna() & pc["PCSTRESC"].str.strip().str.startswith("<")
pc = pc.sort_values(["USUBJID", "ADTM", "SRCSEQ"], kind="stable").reset_index(drop=True)

quantified = pc["RESULT"].notna()
previous_quantified = quantified.groupby(pc["USUBJID"]).shift(1)
previous_quantified = previous_quantified.where(previous_quantified.notna(), False).astype(bool)

conc = pd.DataFrame({
    "STUDYID": pc["STUDYID"],
    "USUBJID": pc["USUBJID"],
    "ADTM": pc["ADTM"],
    "EVID": 0,
    "AMT": np.nan,
    "ALLOQ": pc["ALLOQ"],
    "CENS": pc["BLQ"].astype(int),
    "DV": pc["RESULT"].where(~pc["BLQ"], pc["ALLOQ"]),
    "DVM6": pc["RESULT"].where(
        ~(pc["BLQ"] & previous_quantified), pc["ALLOQ"] / 2
    ),
    "SRCSEQ": pc["SRCSEQ"],
})

adppk = pd.concat([conc, dose], ignore_index=True)
adppk = adppk.sort_values(
    ["USUBJID", "ADTM", "EVID", "SRCSEQ"], kind="stable"
).reset_index(drop=True)
adppk["ASEQ"] = adppk.groupby("USUBJID").cumcount() + 1
adppk["MDV"] = adppk["DV"].isna().astype(int)
adppk["MDVM6"] = adppk["DVM6"].isna().astype(int)

for column in ["AMT", "ALLOQ", "DV", "DVM6"]:
    adppk[column] = adppk[column].map(number)

adppk[COLUMNS].to_csv("/app/output/adppk.csv", index=False, lineterminator="\n")
