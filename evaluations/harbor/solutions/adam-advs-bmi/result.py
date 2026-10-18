# Reference solution for the yamaa benchmark adam-advs-bmi (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

# Baseline height (cm) per subject from ADSL.
baseline_height_cm = {}
with open(f"{INPUT_DIR}/adsl.csv", newline="") as f:
    for row in csv.DictReader(f):
        h = (row.get("HEIGHTBL") or "").strip()
        baseline_height_cm[row["USUBJID"]] = float(h) if h else None

rows = []
with open(f"{INPUT_DIR}/vs.csv", newline="") as f:
    for rec in csv.DictReader(f):
        result = (rec.get("VSSTRESN") or "").strip()
        rows.append(
            {
                "STUDYID": rec["STUDYID"],
                "USUBJID": rec["USUBJID"],
                "AVISIT": rec["VISIT"],
                "PARAMCD": rec["VSTESTCD"],
                "PARAM": rec["VSTEST"],
                "AVAL": result,
            }
        )
        # A BMI record follows each collected weight record with a result.
        if rec["VSTESTCD"] == "WEIGHT" and result:
            height_cm = baseline_height_cm.get(rec["USUBJID"])
            if height_cm is None or height_cm == 0:
                bmi = ""
            else:
                height_m = height_cm / 100.0
                bmi = repr(float(result) / (height_m * height_m))
            rows.append(
                {
                    "STUDYID": rec["STUDYID"],
                    "USUBJID": rec["USUBJID"],
                    "AVISIT": rec["VISIT"],
                    "PARAMCD": "BMI",
                    "PARAM": "Body Mass Index (kg/m^2)",
                    "AVAL": bmi,
                }
            )

columns = ["STUDYID", "USUBJID", "AVISIT", "PARAMCD", "PARAM", "AVAL"]
with open(f"{OUTPUT_DIR}/advs.csv", "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=columns)
    writer.writeheader()
    writer.writerows(rows)
