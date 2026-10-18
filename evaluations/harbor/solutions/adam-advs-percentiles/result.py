# Reference solution for the yamaa benchmark adam-advs-percentiles (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADVS growth percentiles from ADVS_RAW and LMS_REF.

For each collected BMI or WEIGHT measurement, compute the LMS z-score
using the reference coefficients matched on measurement code, sex, and
age in days, then convert to a percentile via the standard normal CDF.
"""

import csv
import math
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

PARAM_MAP = {
    "BMI": ("BMIPCTL", "BMI-for-Age Percentile"),
    "WEIGHT": ("WGTPCTL", "Weight-for-Age Percentile"),
}


def norm_cdf(z):
    return 0.5 * (1.0 + math.erf(z / math.sqrt(2.0)))


def main():
    with open(os.path.join(INPUT_DIR, "lms_ref.csv"), newline="") as f:
        ref = {}
        for row in csv.DictReader(f):
            key = (row["PARAMCD"], row["SEX"], int(row["AGE"]))
            ref[key] = (float(row["L"]), float(row["M"]), float(row["S"]))

    with open(os.path.join(INPUT_DIR, "advs_raw.csv"), newline="") as f:
        raw_rows = list(csv.DictReader(f))

    out_rows = []
    for row in raw_rows:
        code = row["PARAMCD"]
        paramcd, param = PARAM_MAP[code]
        aval = ""
        value_text = (row["AVAL"] or "").strip()
        coeffs = ref.get((code, row["SEX"], int(row["AGE"])))
        if value_text != "" and coeffs is not None:
            x = float(value_text)
            L, M, S = coeffs
            z = ((x / M) ** L - 1.0) / (L * S)
            aval = 100.0 * norm_cdf(z)
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AVISIT": row["AVISIT"],
                "PARAMCD": paramcd,
                "PARAM": param,
                "AVAL": aval,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "advs.csv"), "w", newline="") as f:
        writer = csv.DictWriter(
            f, fieldnames=["STUDYID", "USUBJID", "AVISIT", "PARAMCD", "PARAM", "AVAL"]
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
