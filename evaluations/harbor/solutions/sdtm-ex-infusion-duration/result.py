# Reference solution for the yamaa benchmark sdtm-ex-infusion-duration (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM EX dataset from EX_RAW.

One record per administration. EXDUR writes the collected duration in whole
minutes as ISO 8601 duration text, with a day as the largest part and every
part that is zero left out.
"""

import csv

INPUT_PATH = "/app/input/ex_raw.csv"
OUTPUT_PATH = "/app/output/ex.csv"

COLUMNS = ["DOMAIN", "STUDYID", "USUBJID", "EXSEQ", "EXTRT", "EXSTDTC", "EXDUR"]


def duration_text(minutes):
    """Whole minutes as ISO 8601 duration text: zero minutes is PT0S."""
    if minutes == 0:
        return "PT0S"
    days, rest = divmod(minutes, 24 * 60)
    hours, mins = divmod(rest, 60)
    text = "P"
    if days:
        text += f"{days}D"
    if hours or mins:
        text += "T"
        if hours:
            text += f"{hours}H"
        if mins:
            text += f"{mins}M"
    return text


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as f:
        raw_rows = list(csv.DictReader(f))

    out_rows = []
    for row in raw_rows:
        collected = (row["EXDURMIN"] or "").strip()
        out_rows.append(
            {
                "DOMAIN": "EX",
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "EXSEQ": row["EXSEQ"],
                "EXTRT": row["EXTRT"],
                "EXSTDTC": row["EXSTDTC"],
                # No collected duration leaves EXDUR without a value.
                "EXDUR": duration_text(int(collected)) if collected else "",
            }
        )

    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
