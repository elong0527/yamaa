# Reference solution for the yamaa benchmark adam-adsl-age-group (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_CSV = "/app/input/dm.csv"
OUTPUT_CSV = "/app/output/adsl.csv"


def age_group(age_text):
    """Return (AGEGR1, AGEGR1N) for a raw AGE cell."""
    if age_text is None or age_text.strip() == "":
        return "Missing", ""
    age = float(age_text)
    if age < 18:
        return "<18", "1"
    if age <= 64:
        return "18-64", "2"
    return ">64", "3"


def main():
    with open(INPUT_CSV, newline="") as f:
        rows = list(csv.DictReader(f))

    out_rows = []
    for row in rows:
        age = row["AGE"]
        agegr1, agegr1n = age_group(age)
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AGE": age,
                "AGEU": row["AGEU"],
                "AGEGR1": agegr1,
                "AGEGR1N": agegr1n,
            }
        )

    fieldnames = ["STUDYID", "USUBJID", "AGE", "AGEU", "AGEGR1", "AGEGR1N"]
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
