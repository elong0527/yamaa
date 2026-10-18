# Reference solution for the yamaa benchmark sdtm-dm-death (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM DM dataset (one record per subject) with death date/flag.

Death date (DTHDTC) derivation, in priority order:
1. Latest DSSTDTC among the subject's disposition records whose decoded
   term (DSDECOD) is "DEATH" and that carry a start date.
2. Otherwise, latest AEENDTC among the subject's adverse events whose
   outcome (AEOUT) is "FATAL" and that carry an end date.
3. Otherwise, no value.

DTHFL is "Y" when either source supplies a death date, else blank.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(name):
    return pd.read_csv(f"{INPUT_DIR}/{name}", dtype=str, keep_default_na=False)


def latest_date_by_subject(df, subject_col, date_col, mask):
    """Return {USUBJID: latest non-blank date string} for masked rows."""
    selected = df.loc[mask & (df[date_col].str.strip() != "")]
    result = {}
    for usubjid, dates in selected.groupby(subject_col)[date_col]:
        # ISO 8601 date text sorts chronologically.
        result[usubjid] = max(dates)
    return result


def main():
    dm_raw = read_csv("dm_raw.csv")
    ds = read_csv("ds.csv")
    ae = read_csv("ae.csv")

    ds_death = latest_date_by_subject(
        ds, "USUBJID", "DSSTDTC", ds["DSDECOD"] == "DEATH"
    )
    ae_fatal = latest_date_by_subject(
        ae, "USUBJID", "AEENDTC", ae["AEOUT"] == "FATAL"
    )

    rows = []
    for _, subject in dm_raw.iterrows():
        usubjid = subject["USUBJID"]
        dthdtc = ds_death.get(usubjid) or ae_fatal.get(usubjid) or ""
        rows.append(
            {
                "DOMAIN": "DM",
                "STUDYID": subject["STUDYID"],
                "USUBJID": usubjid,
                "DTHDTC": dthdtc,
                "DTHFL": "Y" if dthdtc else "",
            }
        )

    dm = pd.DataFrame(
        rows, columns=["DOMAIN", "STUDYID", "USUBJID", "DTHDTC", "DTHFL"]
    )
    dm.to_csv(f"{OUTPUT_DIR}/dm.csv", index=False)


if __name__ == "__main__":
    main()
