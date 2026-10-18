# Reference solution for the yamaa benchmark sdtm-mh-ongoing-conditions (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM MH dataset from the collected MH_FORM dataset.

One output record per condition reported on the form, with columns:
STUDYID, USUBJID, MHSEQ, MHTERM, MHSTDTC, MHENDTC, MHENRTPT, MHENTPT.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "MHSEQ",
    "MHTERM",
    "MHSTDTC",
    "MHENDTC",
    "MHENRTPT",
    "MHENTPT",
]


def start_dtc(year, month):
    """Start date at the precision recorded: year, or year and month."""
    year = (year or "").strip()
    month = (month or "").strip()
    if not year:
        return ""
    if month:
        return f"{year}-{int(month):02d}"
    return year


def main():
    with open(f"{INPUT_DIR}/mh_form.csv", newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    out = []
    for row in rows:
        ongoing = (row.get("MHONGO") or "").strip() == "Y"
        end_dtc = (row.get("MHENDTC") or "").strip()
        screen_dtc = (row.get("SCRFDTC") or "").strip()

        if ongoing:
            # A condition still active at screening carries no end date;
            # the ongoing answer is the record of its end status.
            mh_end = ""
            enrtpt = "ONGOING"
        else:
            mh_end = end_dtc
            enrtpt = ""
            if end_dtc and screen_dtc:
                end_date = date.fromisoformat(end_dtc)
                screen_date = date.fromisoformat(screen_dtc)
                if end_date < screen_date:
                    enrtpt = "BEFORE"

        entpt = "SCREENING" if enrtpt else ""

        out.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "MHSEQ": row["MHSEQ"],
                "MHTERM": row["MHTERM"],
                "MHSTDTC": start_dtc(row.get("MHSTYY"), row.get("MHSTMM")),
                "MHENDTC": mh_end,
                "MHENRTPT": enrtpt,
                "MHENTPT": entpt,
            }
        )

    with open(f"{OUTPUT_DIR}/mh.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
