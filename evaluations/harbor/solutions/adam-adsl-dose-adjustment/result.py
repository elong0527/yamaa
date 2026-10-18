# Reference solution for the yamaa benchmark adam-adsl-dose-adjustment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(name):
    path = os.path.join(INPUT_DIR, name)
    with open(path, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def filled(value):
    return value is not None and value.strip() != ""


def main():
    subjects = read_csv("adsl.csv")
    ex = read_csv("ex.csv")
    ec = read_csv("ec.csv")
    fa = read_csv("fa.csv")

    has_record = set()
    qualifying = set()

    for row in ex:
        key = (row["STUDYID"], row["USUBJID"])
        has_record.add(key)
        if filled(row.get("EXADJ")):
            qualifying.add(key)

    for row in ec:
        key = (row["STUDYID"], row["USUBJID"])
        has_record.add(key)
        if filled(row.get("ECADJ")):
            qualifying.add(key)

    for row in fa:
        key = (row["STUDYID"], row["USUBJID"])
        has_record.add(key)
        if (
            row.get("FATESTCD") == "OCCUR"
            and row.get("FAOBJ") == "DOSE ADJUSTMENT"
            and row.get("FASTRESC") == "Y"
        ):
            qualifying.add(key)

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    out_path = os.path.join(OUTPUT_DIR, "adsl.csv")
    with open(out_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(["STUDYID", "USUBJID", "DOSADJFL"])
        for subj in subjects:
            key = (subj["STUDYID"], subj["USUBJID"])
            if key in qualifying:
                flag = "Y"
            elif key in has_record:
                flag = "N"
            else:
                flag = ""
            writer.writerow([subj["STUDYID"], subj["USUBJID"], flag])


if __name__ == "__main__":
    main()
