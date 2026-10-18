# Reference solution for the yamaa benchmark sdtm-tu-tumor-identification (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM TU dataset from TU_RAW.

One record per identified tumor or lesion. Reads /app/input/tu_raw.csv
and writes /app/output/tu.csv.
"""

import csv
import os

INPUT_PATH = "/app/input/tu_raw.csv"
OUTPUT_PATH = "/app/output/tu.csv"

CATEGORY_MAP = {
    "target": "TARGET",
    "non-target": "NON-TARGET",
    "new": "NEW",
}

LINK_PREFIX = {
    "TARGET": "T",
    "NON-TARGET": "NT",
    "NEW": "NEW",
}

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "TUSEQ",
    "TULNKID",
    "TUTESTCD",
    "TUTEST",
    "TUORRES",
    "TUSTRESC",
    "TULOC",
    "TULAT",
    "TUMETHOD",
    "TUEVAL",
    "VISITNUM",
    "VISIT",
    "TUDTC",
]


def main():
    with open(INPUT_PATH, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    out = []
    seq_by_subject = {}
    for row in rows:
        subj = row["USUBJID"]
        seq_by_subject[subj] = seq_by_subject.get(subj, 0) + 1

        category = CATEGORY_MAP[row["LESION_CATEGORY"].strip().lower()]
        link_id = LINK_PREFIX[category] + row["LESION_NUM"].strip()
        laterality = row["LATERALITY"].strip()

        out.append(
            {
                "DOMAIN": "TU",
                "STUDYID": row["STUDYID"],
                "USUBJID": subj,
                "TUSEQ": seq_by_subject[subj],
                "TULNKID": link_id,
                "TUTESTCD": "TUMIDENT",
                "TUTEST": "Tumor Identification",
                "TUORRES": category,
                "TUSTRESC": category,
                "TULOC": row["LOCATION"].strip().upper(),
                "TULAT": laterality.upper() if laterality else "",
                "TUMETHOD": row["METHOD"],
                "TUEVAL": row["EVALUATOR"],
                "VISITNUM": row["VISITNUM"],
                "VISIT": row["VISIT"],
                "TUDTC": row["TUDTC"],
            }
        )

    os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
