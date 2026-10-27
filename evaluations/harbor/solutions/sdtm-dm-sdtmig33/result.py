from pathlib import Path

import pandas as pd


def main():
    source = pd.read_csv(
        Path("/app/input/dm_raw.csv"), dtype=str, keep_default_na=False
    )
    dm = source.copy()
    dm["DOMAIN"] = "DM"
    dm["AGEU"] = "YEARS"
    columns = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "SUBJID",
        "RFSTDTC",
        "SITEID",
        "AGE",
        "AGEU",
        "SEX",
        "RACE",
        "COUNTRY",
    ]
    dm = dm.loc[:, columns].drop_duplicates()
    if dm["USUBJID"].duplicated().any():
        raise ValueError("Conflicting source records for a subject")
    output = Path("/app/output")
    output.mkdir(exist_ok=True)
    dm.to_csv(output / "dm.csv", index=False, na_rep="")


if __name__ == "__main__":
    main()
