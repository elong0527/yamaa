# Reference solution for the yamaa benchmark adam-adppk-baseline-covariates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import numpy as np
import pandas as pd

IN = "/app/input"
OUT = "/app/output"

KEYS = ["STUDYID", "USUBJID"]
COLUMNS = [
    "STUDYID", "USUBJID", "ASEQ", "ADTM", "EVID", "AGE", "SEX", "SEXN",
    "RACE", "RACEN", "WTBL", "HTBL", "BMIBL", "BSABL", "CREATBL", "ALTBL",
    "TBILBL", "CRCLBL", "EGFRBL",
]


def read(name):
    return pd.read_csv(f"{IN}/{name}.csv", dtype=str, keep_default_na=False)


def full_datetime(value):
    value = value.strip()
    if "T" not in value:
        date, time = value, ""
    else:
        date, time = value.split("T", 1)
    parts = [p for p in time.split(":") if p != ""]
    parts = (parts + ["00", "00", "00"])[:3]
    return f"{date}T{':'.join(parts)}"


def baseline(df, testcd, flag, result, tests):
    df = df[(df[flag].str.strip() == "Y") & df[testcd].str.strip().isin(tests)].copy()
    df["_value"] = pd.to_numeric(df[result].str.strip().replace("", np.nan))
    df["_name"] = df[testcd].str.strip().map(tests)
    wide = df.groupby(KEYS + ["_name"], sort=False)["_value"].first().unstack("_name")
    wide = wide.reindex(columns=list(tests.values()))
    wide.columns.name = None
    return wide.reset_index()


adsl = read("adsl")
ex = read("ex")
pc = read("pc")
vs = read("vs")
lb = read("lb")

doses = ex[KEYS].copy()
doses["ADTM"] = ex["EXSTDTC"].map(full_datetime)
doses["EVID"] = 1

samples = pc[KEYS].copy()
samples["ADTM"] = pc["PCDTC"].map(full_datetime)
samples["EVID"] = 0

events = pd.concat([doses, samples], ignore_index=True)
events = events.sort_values(
    KEYS + ["ADTM", "EVID"], ascending=[True, True, True, False], kind="stable"
).reset_index(drop=True)
events["ASEQ"] = events.groupby(KEYS, sort=False).cumcount() + 1

cov = adsl[KEYS + ["AGE", "SEX", "RACE"]].copy()
cov["AGE"] = pd.to_numeric(cov["AGE"].str.strip().replace("", np.nan))
cov["SEXN"] = cov["SEX"].map({"M": 1, "F": 2}).astype("Int64")
cov["RACEN"] = (
    cov["RACE"]
    .map({"ASIAN": 2, "BLACK OR AFRICAN AMERICAN": 3, "WHITE": 5})
    .fillna(6)
    .astype("Int64")
)

cov = cov.merge(
    baseline(vs, "VSTESTCD", "VSBLFL", "VSSTRESN", {"WEIGHT": "WTBL", "HEIGHT": "HTBL"}),
    on=KEYS, how="left",
)
cov = cov.merge(
    baseline(lb, "LBTESTCD", "LBBLFL", "LBSTRESN",
             {"CREAT": "CREATBL", "ALT": "ALTBL", "BILI": "TBILBL"}),
    on=KEYS, how="left",
)

male = cov["SEX"] == "M"
female = cov["SEX"] == "F"
known = male | female

cov["BMIBL"] = cov["WTBL"] / (cov["HTBL"] / 100) ** 2
cov["BSABL"] = np.sqrt(cov["HTBL"] * cov["WTBL"] / 3600)

cg = pd.Series(np.where(male, 1.23, 1.04), index=cov.index).where(known)
cov["CRCLBL"] = (140 - cov["AGE"]) * cov["WTBL"] * cg / cov["CREATBL"]

kappa = pd.Series(np.where(male, 0.9, 0.7), index=cov.index).where(known)
ratio = cov["CREATBL"] / 88.42 / kappa
low = pd.Series(np.where(male, -0.302, -0.241), index=cov.index)
exponent = pd.Series(np.where(ratio > 1, -1.2, low), index=cov.index)
sex_factor = pd.Series(np.where(female, 1.012, 1.0), index=cov.index)
cov["EGFRBL"] = 142 * ratio ** exponent * 0.9938 ** cov["AGE"] * sex_factor

if cov["AGE"].notna().all() and (cov["AGE"] % 1 == 0).all():
    cov["AGE"] = cov["AGE"].astype("Int64")

adppk = events.merge(cov, on=KEYS, how="left")[COLUMNS]
adppk.to_csv(f"{OUT}/adppk.csv", index=False)
