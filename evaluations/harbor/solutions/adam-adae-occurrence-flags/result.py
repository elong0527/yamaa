# Reference solution for the yamaa benchmark adam-adae-occurrence-flags (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADAE with occurrence flags from ADAE_RAW.

One record per adverse event. Treatment-emergent events (TRTEMFL = "Y")
are flagged at three levels:

- AOCCFL:  the subject's first treatment-emergent event.
- AOCCSFL: the subject's first treatment-emergent event within each
  body system.
- AOCCPFL: the subject's first treatment-emergent event within each
  dictionary-derived term, counted inside its body system.

The first event at each level is the treatment-emergent event with the
earliest ASTDT; a lower AESEQ breaks a tie between events on the same
date. Non-treatment-emergent events are never flagged, and subjects
with no treatment-emergent events have no flags at all.
"""

import csv

INPUT_PATH = "/app/input/adae_raw.csv"
OUTPUT_PATH = "/app/output/adae.csv"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AEBODSYS",
    "AEDECOD",
    "ASTDT",
    "TRTEMFL",
    "AOCCFL",
    "AOCCSFL",
    "AOCCPFL",
]


def main() -> None:
    with open(INPUT_PATH, newline="") as handle:
        rows = list(csv.DictReader(handle))

    for row in rows:
        row["AOCCFL"] = ""
        row["AOCCSFL"] = ""
        row["AOCCPFL"] = ""

    # Rank treatment-emergent events by start date, then sequence.
    emergent = [row for row in rows if row["TRTEMFL"] == "Y"]
    emergent.sort(key=lambda row: (row["USUBJID"], row["ASTDT"], int(row["AESEQ"])))

    def flag_first(key_of, flag_column: str) -> None:
        seen = set()
        for row in emergent:
            key = key_of(row)
            if key not in seen:
                seen.add(key)
                row[flag_column] = "Y"

    flag_first(lambda row: row["USUBJID"], "AOCCFL")
    flag_first(lambda row: (row["USUBJID"], row["AEBODSYS"]), "AOCCSFL")
    flag_first(
        lambda row: (row["USUBJID"], row["AEBODSYS"], row["AEDECOD"]),
        "AOCCPFL",
    )

    with open(OUTPUT_PATH, "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        for row in rows:
            writer.writerow({column: row[column] for column in COLUMNS})


if __name__ == "__main__":
    main()
