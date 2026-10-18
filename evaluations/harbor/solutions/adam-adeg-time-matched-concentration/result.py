# Reference solution for the yamaa benchmark adam-adeg-time-matched-concentration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADEG with time-matched PK concentrations from ADEG_RAW and ADPC.

ECGPCFL flags ECG records whose subject, period, period day, and nominal
time point hold a parent-drug (DRGX) sample in ADPC; a below-limit sample
still matches, a metabolite or a sample at another time point never does.
PKCONC is the matching sample's concentration as recorded; it has no
value for a below-limit result or when no sample matches, except that
placebo records carry 0 whenever no quantified result exists. CQTANLFL
flags records holding both a change from baseline and a concentration.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "STUDYID", "USUBJID", "TRTA", "PARAMCD", "PARAM", "APERIOD",
    "APERDAY", "ATPT", "ATPTN", "AVAL", "AVALU", "ABLFL", "BASE", "CHG",
    "BASETYPE", "ECGPCFL", "PKCONC", "CQTANLFL",
]

# A concentration matches an ECG record on subject, period, period day,
# and nominal time point.
KEY = ["STUDYID", "USUBJID", "APERIOD", "APERDAY", "ATPTN"]


def main() -> None:
    adeg = pd.read_csv(
        f"{INPUT_DIR}/adeg_raw.csv", dtype=str, keep_default_na=False
    )
    adpc = pd.read_csv(
        f"{INPUT_DIR}/adpc.csv", dtype=str, keep_default_na=False
    )

    # Only parent-drug samples can match; metabolite results never do.
    samples = {}
    for _, sample in adpc[adpc["PARAMCD"] == "DRGX"].iterrows():
        samples[tuple(sample[k] for k in KEY)] = sample

    ecgpcfl, pkconc, cqtanlfl = [], [], []
    for _, row in adeg.iterrows():
        sample = samples.get(tuple(row[k] for k in KEY))
        matched = sample is not None
        quantified = (
            matched and sample["LLOQFL"] != "Y" and sample["AVAL"] != ""
        )

        # A below-limit sample still matches, so the flag is set.
        ecgpcfl.append("Y" if matched else "")
        if quantified:
            # The concentration as recorded, not rounded, on any treatment.
            pkconc.append(sample["AVAL"])
        elif row["TRTA"] == "Placebo":
            # Placebo records carry 0 whenever no quantified result
            # exists, on the baseline day as on any other day.
            pkconc.append("0")
        else:
            pkconc.append("")
        has_concentration = pkconc[-1] != ""
        cqtanlfl.append(
            "Y" if row["CHG"] != "" and has_concentration else ""
        )

    adeg["ECGPCFL"] = ecgpcfl
    adeg["PKCONC"] = pkconc
    adeg["CQTANLFL"] = cqtanlfl
    adeg[COLUMNS].to_csv(f"{OUTPUT_DIR}/adeg.csv", index=False)


if __name__ == "__main__":
    main()
