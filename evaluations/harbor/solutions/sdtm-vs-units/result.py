# Reference solution for the yamaa benchmark sdtm-vs-units (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM VS dataset from VS_RAW with unit standardization.

Reads /app/input/vs_raw.csv and writes /app/output/vs.csv with one
record per collected vital-signs measurement. Standard units: cm for
height, kg for weight, C for temperature.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

STANDARD_UNIT = {"HEIGHT": "cm", "WEIGHT": "kg", "TEMP": "C"}
LB_TO_KG = 0.45359237

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VISIT", "VSTESTCD",
    "VSTEST", "VSORRES", "VSORRESU", "VSSTRESN", "VSSTRESC",
    "VSSTRESU", "VSSTAT",
]


def format_number(value):
    """Write a number as text, dropping trailing zeros (72.50 -> 72.5)."""
    rounded = round(value, 10)
    if rounded == int(rounded):
        return str(int(rounded))
    return repr(rounded)


def standardize(testcd, value, unit):
    """Return the result as a number in the test's standard unit."""
    if testcd == "WEIGHT" and unit == "LB":
        return value * LB_TO_KG
    if testcd == "TEMP" and unit == "F":
        return (value - 32) * 5 / 9
    return value


def main():
    with open(os.path.join(INPUT_DIR, "vs_raw.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        collected = row["VSORRES"].strip()
        testcd = row["VSTESTCD"]
        if collected:
            value = float(collected)
            std_value = standardize(testcd, value, row["VSORRESU"])
            orres = format_number(value)
            orresu = row["VSORRESU"]
            stresn = format_number(std_value)
            stresc = stresn
            stresu = STANDARD_UNIT[testcd]
            stat = ""
        else:
            orres = ""
            orresu = ""
            stresn = ""
            stresc = ""
            stresu = ""
            stat = "NOT DONE"
        out_rows.append({
            "DOMAIN": "VS",
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "VSSEQ": row["VSSEQ"],
            "VISIT": row["VISIT"],
            "VSTESTCD": testcd,
            "VSTEST": row["VSTEST"],
            "VSORRES": orres,
            "VSORRESU": orresu,
            "VSSTRESN": stresn,
            "VSSTRESC": stresc,
            "VSSTRESU": stresu,
            "VSSTAT": stat,
        })

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "vs.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
