# Reference solution for the yamaa benchmark adam-adppk-event-records (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import pandas as pd

COLUMNS = ["STUDYID", "USUBJID", "ID", "ASEQ", "ADTM", "EVID", "CMT", "PARAMCD", "AMT", "DV", "MDV"]
CMT = {"DRGX": 2, "MET1": 3}


def read(name):
    frame = pd.read_csv(f"/app/input/{name}.csv", dtype=str, keep_default_na=False)
    return frame.apply(lambda column: column.str.strip())


def adtm(values):
    return pd.to_datetime(values, format="ISO8601").dt.strftime("%Y-%m-%dT%H:%M:%S")


ex = read("ex")
ex = ex[pd.to_numeric(ex["EXDOSE"], errors="coerce") > 0]
dose = pd.DataFrame(
    {
        "STUDYID": ex["STUDYID"],
        "USUBJID": ex["USUBJID"],
        "ADTM": adtm(ex["EXSTDTC"]),
        "EVID": 1,
        "CMT": 1,
        "PARAMCD": "DOSE",
        "AMT": ex["EXDOSE"],
        "DV": "",
        "SEQ": pd.to_numeric(ex["EXSEQ"], errors="coerce"),
    }
)

pc = read("pc")
pc = pc[(pc["PCSTAT"].str.upper() != "NOT DONE") & pc["USUBJID"].isin(dose["USUBJID"])]
conc = pd.DataFrame(
    {
        "STUDYID": pc["STUDYID"],
        "USUBJID": pc["USUBJID"],
        "ADTM": adtm(pc["PCDTC"]),
        "EVID": 0,
        "CMT": pc["PCTESTCD"].map(CMT).astype("Int64"),
        "PARAMCD": pc["PCTESTCD"],
        "AMT": "",
        "DV": pc["PCSTRESN"],
        "SEQ": pd.to_numeric(pc["PCSEQ"], errors="coerce"),
    }
)

adppk = pd.concat([dose, conc], ignore_index=True)
adppk["CMT"] = adppk["CMT"].astype("Int64")
adppk["MDV"] = (adppk["DV"] == "").astype(int)
adppk = adppk.sort_values(["USUBJID", "ADTM", "EVID", "CMT", "SEQ"], kind="stable").reset_index(drop=True)
adppk["ID"] = adppk.groupby("USUBJID", sort=True).ngroup() + 1
adppk["ASEQ"] = adppk.groupby("USUBJID").cumcount() + 1
adppk[COLUMNS].to_csv("/app/output/adppk.csv", index=False)
