# Reference solution for the yamaa benchmark adam-adppk-relative-times (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import numpy as np
import pandas as pd

COLUMNS = ["STUDYID", "USUBJID", "ASEQ", "ADTM", "EVID", "AMT", "FANLDTM", "AFRLT", "APRLT", "DOSEA"]


def read(name):
    return pd.read_csv(f"/app/input/{name}.csv", dtype=str, keep_default_na=False)


def stamp(series):
    return pd.to_datetime(series.str.strip().replace("", None), format="ISO8601")


def hours(later, earlier):
    return (later - earlier) / pd.Timedelta(hours=1)


ex = read("ex")
ex["AMT"] = pd.to_numeric(ex["EXDOSE"].str.strip().replace("", None))
dose = ex[ex["AMT"] > 0].copy()
dose["ADTM"] = stamp(dose["EXSTDTC"])
dose["EVID"] = 1
dose["SEQ"] = pd.to_numeric(dose["EXSEQ"])

pc = read("pc")
conc = pc.copy()
conc["ADTM"] = stamp(conc["PCDTC"])
conc["EVID"] = 0
conc["AMT"] = np.nan
conc["SEQ"] = pd.to_numeric(conc["PCSEQ"])

keep = ["STUDYID", "USUBJID", "ADTM", "EVID", "AMT", "SEQ"]
adppk = pd.concat([dose[keep], conc[keep]], ignore_index=True)
adppk = adppk.sort_values(["USUBJID", "ADTM", "EVID", "SEQ"], kind="stable", na_position="last").reset_index(drop=True)
adppk["ASEQ"] = adppk.groupby("USUBJID").cumcount() + 1

dosed = dose.dropna(subset=["ADTM"]).sort_values(["USUBJID", "ADTM", "SEQ"], kind="stable")
first = dosed.groupby("USUBJID", sort=False).first()
adppk["FANLDTM"] = adppk["USUBJID"].map(first["ADTM"])
adppk["AFRLT"] = hours(adppk["ADTM"], adppk["FANLDTM"])

aprlt, dosea = [], []
by_subject = {subject: rows for subject, rows in dosed.groupby("USUBJID", sort=False)}
for row in adppk.itertuples(index=False):
    if row.EVID == 1:
        aprlt.append(0.0)
        dosea.append(row.AMT)
        continue
    doses = by_subject.get(row.USUBJID)
    if doses is None or pd.isna(row.ADTM):
        aprlt.append(np.nan)
        dosea.append(np.nan if doses is None else doses["AMT"].iloc[0])
        continue
    before = doses[doses["ADTM"] < row.ADTM]
    if before.empty:
        aprlt.append(row.AFRLT)
        dosea.append(doses["AMT"].iloc[0])
    else:
        aprlt.append((row.ADTM - before["ADTM"].iloc[-1]) / pd.Timedelta(hours=1))
        dosea.append(before["AMT"].iloc[-1])
adppk["APRLT"] = aprlt
adppk["DOSEA"] = dosea

for column in ("ADTM", "FANLDTM"):
    adppk[column] = adppk[column].dt.strftime("%Y-%m-%dT%H:%M:%S")

adppk[COLUMNS].to_csv("/app/output/adppk.csv", index=False)
