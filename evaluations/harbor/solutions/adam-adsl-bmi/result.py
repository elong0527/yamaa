# Reference solution for the yamaa benchmark adam-adsl-bmi (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADSL dataset with BMI derived from height and weight.

Reads the ADSL dataset from /app/input and writes /app/output/adsl.csv
with one record per subject and the columns STUDYID, USUBJID, HEIGHTCM,
WEIGHTKG, BMI, BMI_FN.

BMI is WEIGHTKG divided by the square of the height in metres
(HEIGHTCM / 100). BMI has no value when HEIGHTCM has no value or is zero,
or when WEIGHTKG has no value. A WEIGHTKG of zero gives a BMI of zero.
BMI_FN is the same body mass index from a project routine and agrees with
BMI on every record. Neither value is rounded.
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


def bmi_fn(height_cm, weight_kg):
    """Project routine for the body mass index.

    Has no value when HEIGHTCM or WEIGHTKG has no value, and agrees with
    BMI on every record.
    """
    if height_cm is None or weight_kg is None:
        return None
    if height_cm == 0:
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

    columns = ["STUDYID", "USUBJID", "HEIGHTCM", "WEIGHTKG", "BMI", "BMI_FN"]
    with open("/app/output/adsl.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        for row in rows:
            height = number(row["HEIGHTCM"])
            weight = number(row["WEIGHTKG"])
            bmi_value = bmi(height, weight)
            bmi_fn_value = bmi_fn(height, weight)
            writer.writerow(
                {
                    "STUDYID": row["STUDYID"],
                    "USUBJID": row["USUBJID"],
                    "HEIGHTCM": row["HEIGHTCM"],
                    "WEIGHTKG": row["WEIGHTKG"],
                    "BMI": "" if bmi_value is None else bmi_value,
                    "BMI_FN": "" if bmi_fn_value is None else bmi_fn_value,
                }
            )


if __name__ == "__main__":
    main()
