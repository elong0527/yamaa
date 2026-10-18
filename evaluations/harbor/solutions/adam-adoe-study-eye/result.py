# Reference solution for the yamaa benchmark adam-adoe-study-eye (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADOE from ADSL and OE.

One record per collected ophthalmic examination measurement. AFEYE gives
the eye's role in the study, derived from the collected laterality
(OELAT) and the subject's assigned eye (STUDYEYE in ADSL):

- "Both Eyes" for a bilateral measurement when the subject has an
  assigned eye.
- "Study Eye" when the measured eye matches the assigned eye; when the
  assigned eye is both eyes, a measurement of either eye is "Study Eye"
  whenever the collected laterality is present.
- "Fellow Eye" when the measured eye is the opposite eye to the
  assigned eye.
- No value when the subject has no assigned eye or the collected
  laterality is missing.

A measurement collected without a value is kept with AVAL missing.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def afeye(study_eye, oelat):
    if not study_eye or not oelat:
        return ""
    if oelat == "BILATERAL":
        return "Both Eyes"
    if study_eye == "BILATERAL":
        return "Study Eye"
    if oelat == study_eye:
        return "Study Eye"
    return "Fellow Eye"


def main():
    adsl = read_csv(os.path.join(INPUT_DIR, "adsl.csv"))
    oe = read_csv(os.path.join(INPUT_DIR, "oe.csv"))

    study_eye_by_subject = {
        (row["STUDYID"], row["USUBJID"]): row["STUDYEYE"] for row in adsl
    }

    columns = ["STUDYID", "USUBJID", "OESEQ", "PARAMCD", "OELAT", "AVAL", "AFEYE"]
    records = []
    for row in oe:
        key = (row["STUDYID"], row["USUBJID"])
        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "OESEQ": row["OESEQ"],
                "PARAMCD": row["OETESTCD"],
                "OELAT": row["OELAT"],
                "AVAL": row["OESTRESN"],
                "AFEYE": afeye(study_eye_by_subject.get(key, ""), row["OELAT"]),
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(
        os.path.join(OUTPUT_DIR, "adoe.csv"), "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
