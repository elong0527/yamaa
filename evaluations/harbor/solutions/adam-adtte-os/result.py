# Reference solution for the yamaa benchmark adam-adtte-os (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADTTE (Overall Survival) from ADRS and ADSL.

One record per subject. Survival time is counted from the
randomization date (RANDDT in ADSL). A qualifying death is an ADRS
record with PARAMCD "DEATH", AVALC "Y", ANL01FL "Y" and a recorded
ADT; the earliest such record (lowest ASEQ on a tie) is the event.
Subjects without a qualifying death are censored at their last-alive
date (LSTALVDT) when it falls after randomization, and at
randomization otherwise.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "PARAMCD",
    "PARAM",
    "STARTDT",
    "ADT",
    "AVAL",
    "CNSR",
    "EVNTDESC",
    "CNSDTDSC",
    "SRCDOM",
    "SRCVAR",
    "SRCSEQ",
]


def parse_date(value):
    """Parse an ISO date string; return a Timestamp or NaT."""
    if value is None or str(value).strip() == "":
        return pd.NaT
    return pd.Timestamp(str(value).strip())


def fmt_date(ts):
    return "" if pd.isna(ts) else ts.strftime("%Y-%m-%d")


def main():
    adsl = pd.read_csv(f"{INPUT_DIR}/adsl.csv", dtype=str, keep_default_na=False)
    adrs = pd.read_csv(f"{INPUT_DIR}/adrs.csv", dtype=str, keep_default_na=False)

    # Qualifying death records per subject.
    deaths = adrs[
        (adrs["PARAMCD"] == "DEATH")
        & (adrs["AVALC"] == "Y")
        & (adrs["ANL01FL"] == "Y")
        & (adrs["ADT"].str.strip() != "")
    ].copy()
    deaths["_adt"] = deaths["ADT"].map(parse_date)
    deaths["_aseq"] = deaths["ASEQ"].astype(int)
    deaths = deaths.sort_values(["_adt", "_aseq"])
    death_by_subject = {}
    for _, row in deaths.iterrows():
        death_by_subject.setdefault(row["USUBJID"], row)

    records = []
    for _, subj in adsl.iterrows():
        studyid = subj["STUDYID"]
        usubjid = subj["USUBJID"]
        startdt = parse_date(subj["RANDDT"])
        lstalvdt = parse_date(subj["LSTALVDT"])

        death = death_by_subject.get(usubjid)
        if death is not None:
            death_adt = death["_adt"]
            adt = death_adt if death_adt >= startdt else startdt
            cnsr = 0
            evntdesc = "Death"
            cnsdtdsc = ""
            srcdom = "ADRS"
            srcvar = "ADT"
            srcseq = str(death["_aseq"])
        else:
            cnsr = 1
            srcdom = "ADSL"
            srcseq = ""
            if not pd.isna(lstalvdt) and lstalvdt > startdt:
                adt = lstalvdt
                evntdesc = "Alive"
                cnsdtdsc = "Alive During Study"
                srcvar = "LSTALVDT"
            else:
                adt = startdt
                evntdesc = "Randomization"
                cnsdtdsc = "Randomization"
                srcvar = "RANDDT"

        aval = (adt - startdt).days + 1

        records.append(
            {
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "PARAMCD": "OS",
                "PARAM": "Overall Survival",
                "STARTDT": fmt_date(startdt),
                "ADT": fmt_date(adt),
                "AVAL": aval,
                "CNSR": cnsr,
                "EVNTDESC": evntdesc,
                "CNSDTDSC": cnsdtdsc,
                "SRCDOM": srcdom,
                "SRCVAR": srcvar,
                "SRCSEQ": srcseq,
            }
        )

    out = pd.DataFrame(records, columns=COLUMNS)
    out.to_csv(f"{OUTPUT_DIR}/adtte.csv", index=False)


if __name__ == "__main__":
    main()
