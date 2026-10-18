# Reference solution for the yamaa benchmark adam-adsl-country-region (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADSL from DM: one record per subject with COUNTRY and REGION1.

COUNTRY is the collected country in upper case, or "UNKNOWN" when no
country was collected. REGION1 is "North America" for USA/CAN, "Europe"
for DEU, and "Rest of World" for any other country, including "UNKNOWN".
"""

import csv

INPUT_CSV = "/app/input/dm.csv"
OUTPUT_CSV = "/app/output/adsl.csv"

OUTPUT_COLUMNS = ["STUDYID", "USUBJID", "COUNTRY", "REGION1"]


def region_for(country):
    if country in ("USA", "CAN"):
        return "North America"
    if country == "DEU":
        return "Europe"
    return "Rest of World"


def main():
    with open(INPUT_CSV, newline="", encoding="utf-8") as f:
        dm = list(csv.DictReader(f))

    rows = []
    for record in dm:
        collected = (record.get("COUNTRY") or "").strip()
        country = collected.upper() if collected else "UNKNOWN"
        rows.append(
            {
                "STUDYID": record["STUDYID"],
                "USUBJID": record["USUBJID"],
                "COUNTRY": country,
                "REGION1": region_for(country),
            }
        )

    with open(OUTPUT_CSV, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=OUTPUT_COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
