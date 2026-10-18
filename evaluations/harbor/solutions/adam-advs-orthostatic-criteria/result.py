# Reference solution for the yamaa benchmark adam-advs-orthostatic-criteria (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADVS orthostatic change and flag records from the VS dataset.

One record per subject per visit per parameter:
  SYSBPOC / DIABPOC / HROC - change records (3-minute standing minus supine)
  OHYPO / OTACHY           - flag records
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

PARAMS = {
    "SYSBPOC": "Systolic Blood Pressure Change on Standing (mmHg)",
    "DIABPOC": "Diastolic Blood Pressure Change on Standing (mmHg)",
    "HROC": "Heart Rate Change on Standing (beats/min)",
    "OHYPO": "Orthostatic Hypotension",
    "OTACHY": "Orthostatic Tachycardia",
}

# Source test code behind each change parameter.
CHANGE_TESTS = {
    "SYSBPOC": "SYSBP",
    "DIABPOC": "DIABP",
    "HROC": "HR",
}


def parse_number(text):
    """Return the numeric value of a cell, or None when missing."""
    if text is None:
        return None
    text = text.strip()
    if not text:
        return None
    return float(text)


def format_number(value):
    """Write integral values without a decimal point."""
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    with open(os.path.join(INPUT_DIR, "vs.csv"), newline="") as handle:
        rows = list(csv.DictReader(handle))

    # readings[(study, subject, visit, test)] = (supine, standing_3min)
    readings = {}
    visit_order = {}
    subject_order = []
    for row in rows:
        key = (row["STUDYID"], row["USUBJID"])
        if key not in visit_order:
            visit_order[key] = []
            subject_order.append(key)
        if row["VISIT"] not in visit_order[key]:
            visit_order[key].append(row["VISIT"])
        value = parse_number(row["VSSTRESN"])
        if value is None:
            slot_value = None
        else:
            slot_value = value
        entry = readings.setdefault(
            (row["STUDYID"], row["USUBJID"], row["VISIT"], row["VSTESTCD"]),
            {"supine": None, "standing": None},
        )
        if row["VSPOS"] == "SUPINE":
            entry["supine"] = slot_value
        elif row["VSPOS"] == "STANDING" and "3 MIN" in row["VSTPT"]:
            # Only the reading after three minutes standing is used;
            # the one-minute standing reading never counts.
            entry["standing"] = slot_value

    def change(study, subject, visit, test):
        entry = readings.get((study, subject, visit, test))
        if entry is None or entry["supine"] is None or entry["standing"] is None:
            return None
        return entry["standing"] - entry["supine"]

    records = []
    for study, subject in subject_order:
        for visit in visit_order[(study, subject)]:
            changes = {
                param: change(study, subject, visit, test)
                for param, test in CHANGE_TESTS.items()
            }
            for param in ("SYSBPOC", "DIABPOC", "HROC"):
                if changes[param] is not None:
                    records.append(
                        [study, subject, visit, param, PARAMS[param],
                         format_number(changes[param]), ""]
                    )
            # Orthostatic hypotension flag.
            sys_chg = changes["SYSBPOC"]
            dia_chg = changes["DIABPOC"]
            sys_fall = sys_chg is not None and sys_chg <= -20
            dia_fall = dia_chg is not None and dia_chg <= -10
            if sys_fall or dia_fall:
                avalc = "Y"
            elif sys_chg is not None and dia_chg is not None:
                avalc = "N"
            else:
                avalc = None  # neither meets threshold, one change missing
            if avalc is not None:
                records.append(
                    [study, subject, visit, "OHYPO", PARAMS["OHYPO"], "", avalc]
                )
            # Orthostatic tachycardia flag.
            hr_chg = changes["HROC"]
            if hr_chg is not None:
                avalc = "Y" if hr_chg >= 30 else "N"
                records.append(
                    [study, subject, visit, "OTACHY", PARAMS["OTACHY"], "", avalc]
                )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "advs.csv"), "w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(
            ["STUDYID", "USUBJID", "AVISIT", "PARAMCD", "PARAM", "AVAL", "AVALC"]
        )
        writer.writerows(records)


if __name__ == "__main__":
    main()
