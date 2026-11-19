# Reference solution for the yamaa benchmark adam-adppk-time-varying-covariates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import pandas as pd

IN = "/app/input"
OUT = "/app/output/adppk.csv"


def read(name):
    return pd.read_csv(f"{IN}/{name}.csv", dtype=str, keep_default_na=False)


def adtm(value):
    value = value.strip()
    date, _, time = value.partition("T")
    parts = [p for p in time.split(":") if p != ""] if time else []
    parts = (parts + ["00", "00", "00"])[:3]
    parts[2] = parts[2].split(".")[0].zfill(2)
    return f"{date}T{parts[0].zfill(2)}:{parts[1].zfill(2)}:{parts[2]}"


def results(frame, testcd_col, testcd, value_col, date_col, seq_col):
    rows = frame[frame[testcd_col].str.strip() == testcd].copy()
    rows["value"] = pd.to_numeric(rows[value_col].str.strip(), errors="coerce")
    rows = rows[rows["value"].notna()]
    rows["date"] = rows[date_col].str.strip().str[:10]
    rows = rows[rows["date"].str.len() == 10]
    rows["seq"] = pd.to_numeric(rows[seq_col], errors="coerce")
    rows = rows.sort_values(["USUBJID", "date", "seq"], kind="stable")
    return {k: list(zip(g["date"], g["value"])) for k, g in rows.groupby("USUBJID")}


def latest(history, subject, date):
    found = None
    for d, v in history.get(subject, []):
        if d <= date:
            found = v
    return found


def text(value):
    if value is None:
        return ""
    if float(value).is_integer():
        return str(int(value))
    return repr(float(value))


adsl = read("adsl")
ex = read("ex")
pc = read("pc")
vs = read("vs")
lb = read("lb")

weights = results(vs, "VSTESTCD", "WEIGHT", "VSSTRESN", "VSDTC", "VSSEQ")
creats = results(lb, "LBTESTCD", "CREAT", "LBSTRESN", "LBDTC", "LBSEQ")
subjects = {
    r.USUBJID: (pd.to_numeric(r.AGE, errors="coerce"), r.SEX.strip())
    for r in adsl.itertuples()
}

doses = pd.DataFrame({
    "STUDYID": ex["STUDYID"],
    "USUBJID": ex["USUBJID"],
    "ADTM": ex["EXSTDTC"].map(adtm),
    "EVID": 1,
    "SRCSEQ": pd.to_numeric(ex["EXSEQ"], errors="coerce"),
})
samples = pd.DataFrame({
    "STUDYID": pc["STUDYID"],
    "USUBJID": pc["USUBJID"],
    "ADTM": pc["PCDTC"].map(adtm),
    "EVID": 0,
    "SRCSEQ": pd.to_numeric(pc["PCSEQ"], errors="coerce"),
})

adppk = pd.concat([doses, samples], ignore_index=True)
adppk = adppk.sort_values(
    ["STUDYID", "USUBJID", "ADTM", "EVID", "SRCSEQ"], kind="stable"
).reset_index(drop=True)
adppk["ASEQ"] = adppk.groupby("USUBJID").cumcount() + 1

wt, creat, crcl = [], [], []
for r in adppk.itertuples():
    date = r.ADTM[:10]
    w = latest(weights, r.USUBJID, date)
    c = latest(creats, r.USUBJID, date)
    age, sex = subjects.get(r.USUBJID, (None, ""))
    factor = {"M": 1.23, "F": 1.04}.get(sex)
    if w is None or c is None or factor is None or age is None or pd.isna(age):
        cl = None
    else:
        cl = (140 - age) * w * factor / c
    wt.append(text(w))
    creat.append(text(c))
    crcl.append(text(cl))

adppk["WT"] = wt
adppk["CREAT"] = creat
adppk["CRCL"] = crcl

adppk[["STUDYID", "USUBJID", "ASEQ", "ADTM", "EVID", "WT", "CREAT", "CRCL"]].to_csv(
    OUT, index=False, lineterminator="\n"
)
