# Reference solution for the yamaa benchmark adam-adeg-time-matched-concentration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADEG from ADEG_RAW and ADPC.

Adds time-matched pharmacokinetic concentrations from the parent drug
DRGX onto each ECG record. A DRGX sample matches an ECG record only at
the same subject, analysis period, period day, and nominal time point.
Values are carried through exactly as recorded.
"""

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_rows(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


adeg_rows = read_rows(f"{INPUT_DIR}/adeg_raw.csv")
adpc_rows = read_rows(f"{INPUT_DIR}/adpc.csv")

# Index the parent-drug samples by subject, period, period day, and
# nominal time point. Metabolite results never match.
pk_samples = {}
for sample in adpc_rows:
    if sample["PARAMCD"] != "DRGX":
        continue
    key = (
        sample["USUBJID"],
        sample["APERIOD"],
        sample["APERDAY"],
        sample["ATPTN"],
    )
    pk_samples[key] = sample

output_rows = []
for record in adeg_rows:
    key = (
        record["USUBJID"],
        record["APERIOD"],
        record["APERDAY"],
        record["ATPTN"],
    )
    sample = pk_samples.get(key)
    if sample is None:
        flag = ""
        concentration = ""
    else:
        # A matched sample sets the flag even when its result is below
        # the lower limit of quantification, but such a result gives
        # no concentration.
        flag = "Y"
        if sample["LLOQFL"] == "Y" or sample["AVAL"] == "":
            concentration = ""
        else:
            concentration = sample["AVAL"]
    # On placebo records a missing concentration is zero, on the
    # baseline day as on any other day. The record's own treatment
    # decides whether it is a placebo record.
    if concentration == "" and record["TRTA"] == "Placebo":
        concentration = "0"
    # The concentration-QTc analysis flag needs both a change from
    # baseline and a concentration.
    analysis_flag = (
        "Y" if record["CHG"] != "" and concentration != "" else ""
    )
    output_rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "TRTA": record["TRTA"],
            "PARAMCD": record["PARAMCD"],
            "PARAM": record["PARAM"],
            "APERIOD": record["APERIOD"],
            "APERDAY": record["APERDAY"],
            "ATPT": record["ATPT"],
            "ATPTN": record["ATPTN"],
            "AVAL": record["AVAL"],
            "AVALU": record["AVALU"],
            "ABLFL": record["ABLFL"],
            "BASE": record["BASE"],
            "CHG": record["CHG"],
            "BASETYPE": record["BASETYPE"],
            "ECGPCFL": flag,
            "PKCONC": concentration,
            "CQTANLFL": analysis_flag,
        }
    )

columns = [
    "STUDYID",
    "USUBJID",
    "TRTA",
    "PARAMCD",
    "PARAM",
    "APERIOD",
    "APERDAY",
    "ATPT",
    "ATPTN",
    "AVAL",
    "AVALU",
    "ABLFL",
    "BASE",
    "CHG",
    "BASETYPE",
    "ECGPCFL",
    "PKCONC",
    "CQTANLFL",
]
with open(f"{OUTPUT_DIR}/adeg.csv", "w", newline="") as handle:
    writer = csv.DictWriter(handle, fieldnames=columns)
    writer.writeheader()
    writer.writerows(output_rows)
