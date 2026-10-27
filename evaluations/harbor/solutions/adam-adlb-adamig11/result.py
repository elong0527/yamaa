"""Create ADLB from the supplied ADSL and LB data (ADaMIG 1.1)."""

from pathlib import Path

import pandas as pd


def main():
    adsl = pd.read_csv(Path("/app/input") / "adsl.csv", dtype=str)
    lb = pd.read_csv(Path("/app/input") / "lb.csv", dtype=str)

    # The requested analysis parameter is ALT.
    adlb = lb.loc[lb["LBTESTCD"].eq("ALT")].copy()
    adlb = adlb.merge(
        adsl[["STUDYID", "USUBJID", "TRTSDT", "TRT01A"]],
        on=["STUDYID", "USUBJID"],
        how="left",
        validate="many_to_one",
    )
    adlb["PARAMCD"] = "ALT"
    adlb["PARAM"] = "Alanine Aminotransferase"
    collection_date = pd.to_datetime(adlb["LBDTC"].str[:10], errors="coerce")
    treatment_date = pd.to_datetime(adlb["TRTSDT"], errors="coerce")
    adlb["ADT"] = collection_date.dt.strftime("%Y-%m-%d")
    day_difference = (collection_date - treatment_date).dt.days
    # ADaM study day has no day zero.
    adlb["ADY"] = (day_difference + day_difference.ge(0)).astype("Int64")
    adlb["AVAL"] = pd.to_numeric(adlb["LBSTRESN"], errors="coerce")
    adlb["AVALU"] = adlb["LBSTRESU"]
    adlb["ABLFL"] = adlb["LBBLFL"].where(adlb["LBBLFL"].eq("Y"), "")

    keys = ["STUDYID", "USUBJID", "PARAMCD"]
    baseline = adlb.loc[adlb["ABLFL"].eq("Y"), keys + ["AVAL"]].rename(
        columns={"AVAL": "BASE"}
    )
    adlb = adlb.merge(baseline, on=keys, how="left", validate="many_to_one")
    adlb["CHG"] = adlb["AVAL"] - adlb["BASE"]
    columns = [
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "ADT",
        "TRT01A",
        "ADY",
        "AVAL",
        "AVALU",
        "ABLFL",
        "BASE",
        "CHG",
    ]
    adlb = adlb[columns].sort_values(keys + ["ADT"], kind="stable")
    if adlb.duplicated(keys + ["ADT"]).any():
        raise ValueError("More than one ALT record per subject and collection date")
    Path("/app/output").mkdir(exist_ok=True)
    adlb.to_csv(Path("/app/output") / "adlb.csv", index=False)


if __name__ == "__main__":
    main()
