# Reference solution for the yamaa benchmark adam-adsl-bmi (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADSL dataset with baseline BMI from baseline height and weight.

Reads the ADSL dataset from /app/input and writes /app/output/adsl.csv
with one record per subject and the columns STUDYID, USUBJID, HEIGHTBL,
WEIGHTBL, BMIBL.

BMIBL is WEIGHTBL divided by the square of the height in metres
(HEIGHTBL / 100). BMIBL has no value when HEIGHTBL has no value or is
zero, or when WEIGHTBL has no value. A WEIGHTBL of zero gives a BMIBL of
zero. The value is not rounded.
"""

import csv


def bmi(height_cm, weight_kg):
    """Body mass index from height in cm and weight in kg.

    Returns None when the height has no value or is zero, or when the
    weight has no value.
    """
    if height_cm is None or height_cm == 0 or weight_kg is None:
        return None
    height_m = height_cm / 100
    return weight_kg / (height_m * height_m)


def number(text):
    if text is None or text == "":
        return None
    return float(text)


def main():
    with open("/app/input/adsl.csv", newline="") as handle:
        rows = list(csv.DictReader(handle))

    columns = ["STUDYID", "USUBJID", "HEIGHTBL", "WEIGHTBL", "BMIBL"]
    with open("/app/output/adsl.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        for row in rows:
            value = bmi(number(row["HEIGHTBL"]), number(row["WEIGHTBL"]))
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "HEIGHTBL": row["HEIGHTBL"],
                    "WEIGHTBL": row["WEIGHTBL"],
                    "BMIBL": "" if value is None else value,
                }
            )


if __name__ == "__main__":
    main()
