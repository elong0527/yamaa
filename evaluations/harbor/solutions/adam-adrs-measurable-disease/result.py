# Reference solution for the yamaa benchmark adam-adrs-measurable-disease (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = read_csv(f"{INPUT_DIR}/adsl.csv")
    tu = read_csv(f"{INPUT_DIR}/tu.csv")

    target_subjects = set()
    for row in tu:
        if (
            row.get("TUTESTCD") == "TUMIDENT"
            and row.get("VISIT") == "SCREENING"
            and (row.get("TUSTRESC") or "").strip() == "TARGET"
        ):
            target_subjects.add((row.get("STUDYID"), row.get("USUBJID")))

    out_rows = []
    for subject in adsl:
        key = (subject.get("STUDYID"), subject.get("USUBJID"))
        avalc = "Y" if key in target_subjects else "N"
        out_rows.append(
            {
                "STUDYID": subject.get("STUDYID"),
                "USUBJID": subject.get("USUBJID"),
                "PARAMCD": "MDIS",
                "PARAM": "Measurable Disease at Baseline",
                "AVALC": avalc,
                "AVAL": 1 if avalc == "Y" else 0,
            }
        )

    with open(
        f"{OUTPUT_DIR}/adrs.csv", "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVALC", "AVAL"],
        )
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
