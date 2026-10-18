# Reference solution for the yamaa benchmark adam-adae-severity (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from pathlib import Path


INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")
OUTPUT_COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "AEDECOD", "AESEV"]


def main():
    severity_by_event = {}

    with (INPUT_DIR / "supp.csv").open(newline="", encoding="utf-8") as supp_file:
        supp_reader = csv.DictReader(supp_file)
        for supp_row in supp_reader:
            event_key = (
                supp_row["STUDYID"],
                supp_row["USUBJID"],
                supp_row["AESEQ"],
            )
            if event_key not in severity_by_event:
                severity_by_event[event_key] = supp_row.get("AESEV") or ""

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    with (INPUT_DIR / "ae.csv").open(newline="", encoding="utf-8") as ae_file, (
        OUTPUT_DIR / "adae.csv"
    ).open("w", newline="", encoding="utf-8") as output_file:
        ae_reader = csv.DictReader(ae_file)
        writer = csv.DictWriter(output_file, fieldnames=OUTPUT_COLUMNS)
        writer.writeheader()

        for ae_row in ae_reader:
            event_key = (
                ae_row["STUDYID"],
                ae_row["USUBJID"],
                ae_row["AESEQ"],
            )
            writer.writerow(
                {
                    "STUDYID": ae_row["STUDYID"],
                    "USUBJID": ae_row["USUBJID"],
                    "AESEQ": ae_row["AESEQ"],
                    "AEDECOD": ae_row["AEDECOD"],
                    "AESEV": severity_by_event.get(event_key, ""),
                }
            )


if __name__ == "__main__":
    main()
