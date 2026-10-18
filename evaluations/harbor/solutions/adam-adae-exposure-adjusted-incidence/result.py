# Reference solution for the yamaa benchmark adam-adae-exposure-adjusted-incidence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Exposure-adjusted incidence: build ADRISK and ADINC from ADAE and ADSL.

ADRISK: one record per study, subject, and prespecified event term
("DIARRHOEA", "HEADACHE"), with the first in-window onset date, an event
indicator, and days at risk. ADINC: exposure-adjusted incidence per study,
actual treatment, and term.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

TERMS = ["DIARRHOEA", "HEADACHE"]
WINDOW_DAYS_AFTER_LAST_DOSE = 30


def main() -> None:
    adsl = pd.read_csv(f"{INPUT_DIR}/adsl.csv", dtype=str)
    adae = pd.read_csv(f"{INPUT_DIR}/adae.csv", dtype=str)

    adsl["TRTSDT"] = pd.to_datetime(adsl["TRTSDT"])
    adsl["TRTEDT"] = pd.to_datetime(adsl["TRTEDT"])
    if not adae.empty:
        adae["ASTDT"] = pd.to_datetime(adae["ASTDT"])

    rows = []
    for _, subj in adsl.iterrows():
        first_dose = subj["TRTSDT"]
        window_end = subj["TRTEDT"] + pd.Timedelta(
            days=WINDOW_DAYS_AFTER_LAST_DOSE
        )
        subj_ae = adae[adae["USUBJID"] == subj["USUBJID"]] if not adae.empty \
            else adae
        for term in TERMS:
            onsets = subj_ae.loc[
                (subj_ae["AEDECOD"] == term)
                & (subj_ae["ASTDT"] >= first_dose)
                & (subj_ae["ASTDT"] <= window_end),
                "ASTDT",
            ]
            if len(onsets) > 0:
                firstdt = onsets.min()
                eventn = 1
                riskdays = (firstdt - first_dose).days + 1
            else:
                firstdt = pd.NaT
                eventn = 0
                riskdays = (window_end - first_dose).days + 1
            rows.append(
                {
                    "STUDYID": subj["STUDYID"],
                    "USUBJID": subj["USUBJID"],
                    "TRT01A": subj["TRT01A"],
                    "AEDECOD": term,
                    "FIRSTDT": firstdt,
                    "EVENTN": eventn,
                    "RISKDAYS": riskdays,
                }
            )

    adrisk = pd.DataFrame(
        rows,
        columns=[
            "STUDYID", "USUBJID", "TRT01A", "AEDECOD",
            "FIRSTDT", "EVENTN", "RISKDAYS",
        ],
    )
    adrisk = adrisk.sort_values(
        ["STUDYID", "USUBJID", "AEDECOD"]
    ).reset_index(drop=True)
    adrisk_out = adrisk.copy()
    adrisk_out["FIRSTDT"] = adrisk_out["FIRSTDT"].dt.strftime("%Y-%m-%d")
    adrisk_out["FIRSTDT"] = adrisk_out["FIRSTDT"].fillna("")
    adrisk_out.to_csv(f"{OUTPUT_DIR}/adrisk.csv", index=False)

    grouped = (
        adrisk.groupby(["STUDYID", "TRT01A", "AEDECOD"], as_index=False)
        .agg(
            NSUBJ=("EVENTN", "sum"),
            PERSONDAYS=("RISKDAYS", "sum"),
        )
        .sort_values(["STUDYID", "TRT01A", "AEDECOD"])
        .reset_index(drop=True)
    )
    grouped["PERSONYRS"] = grouped["PERSONDAYS"] / 365.25
    grouped["EAIR100"] = 100 * grouped["NSUBJ"] / grouped["PERSONYRS"]
    adinc = grouped[
        ["STUDYID", "TRT01A", "AEDECOD", "NSUBJ",
         "PERSONDAYS", "PERSONYRS", "EAIR100"]
    ]
    adinc.to_csv(f"{OUTPUT_DIR}/adinc.csv", index=False)


if __name__ == "__main__":
    main()
