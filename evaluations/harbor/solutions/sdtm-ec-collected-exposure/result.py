# Reference solution for the yamaa benchmark sdtm-ec-collected-exposure (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM EC dataset from the DOSING_LOG dataset.

One EC record per dosing-log day, per the task instruction.
"""

import csv
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "ECSEQ",
    "ECTRT",
    "ECMOOD",
    "ECOCCUR",
    "ECREASND",
    "ECDOSE",
    "ECDOSU",
    "ECDOSFRM",
    "ECDOSFRQ",
    "ECROUTE",
    "ECSTDTC",
    "ECENDTC",
    "ECADJ",
]


def main() -> None:
    with (INPUT_DIR / "dosing_log.csv").open(newline="") as f:
        log = list(csv.DictReader(f))

    # One record per dosing-log day; sequence each subject's days in
    # date order starting at 1.
    log.sort(key=lambda r: (r["USUBJID"], r["LOGDATE"]))

    records = []
    counters: dict[str, int] = {}
    for row in log:
        subject = row["USUBJID"]
        counters[subject] = counters.get(subject, 0) + 1
        taken = row["DOSE_TAKEN"].strip() == "Y"
        records.append(
            {
                "DOMAIN": "EC",
                "STUDYID": row["STUDYID"],
                "USUBJID": subject,
                "ECSEQ": counters[subject],
                "ECTRT": "Study Drug",
                "ECMOOD": "PERFORMED",
                "ECOCCUR": "Y" if taken else "N",
                "ECREASND": "" if taken else row["MISSREAS"],
                "ECDOSE": row["TABLETS"] if taken else "",
                "ECDOSU": "tablet",
                "ECDOSFRM": "TABLET",
                "ECDOSFRQ": "QD",
                "ECROUTE": "ORAL",
                "ECSTDTC": row["LOGDATE"],
                "ECENDTC": row["LOGDATE"],
                "ECADJ": row["DOSE_ADJ"],
            }
        )

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with (OUTPUT_DIR / "ec.csv").open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
