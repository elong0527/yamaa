from pathlib import Path

import pandas as pd


def main():
    input_path = Path("/app/input") / "visits.csv"
    output_path = Path("/app/output") / "sv.csv"
    visits = pd.read_csv(input_path, dtype=str, keep_default_na=False)
    records = []
    for (studyid, usubjid, visitno), rows in visits.groupby(
        ["STUDYID", "USUBJID", "VISITNO"], sort=False, dropna=False
    ):
        visitnum = float(visitno)
        name = rows["NAME"].iloc[0]
        unplanned = (
            not visitnum.is_integer()
            or "UNSCHEDULED" in name.upper()
            or "UNPLANNED" in name.upper()
        )
        starts = rows.loc[rows["START"].ne(""), "START"]
        ends = rows.loc[rows["END"].ne(""), "END"]
        reasons = list(dict.fromkeys(reason for reason in rows["REASON"] if reason))
        records.append(
            {
                "DOMAIN": "SV",
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "VISITNUM": visitnum,
                "VISIT": name,
                "SVSTDTC": starts.min() if len(starts) else "",
                "SVENDTC": ends.max() if len(ends) else "",
                "SVUPDES": "; ".join(reasons) if unplanned else "",
            }
        )
    columns = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "VISITNUM",
        "VISIT",
        "SVSTDTC",
        "SVENDTC",
        "SVUPDES",
    ]
    sv = pd.DataFrame(records, columns=columns)
    sv = sv.sort_values(["STUDYID", "USUBJID", "VISITNUM"], kind="stable")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    sv.to_csv(output_path, index=False, float_format="%g")


if __name__ == "__main__":
    main()
