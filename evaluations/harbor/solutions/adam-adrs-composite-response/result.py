# Reference solution for the yamaa benchmark adam-adrs-composite-response (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADRS (EASI-75 composite response) from ADRS_RAW and ADSL.

One record per subject and analysis visit from ADRS_RAW, enriched with the
subject-level SAEFL and DCSREAS from ADSL. A subject flagged by the safety
or discontinuation rule is a non-responder whatever the percent change;
otherwise a missing percent change is not evaluable, a percent change of
-75 or less is a responder, and anything else is a non-responder.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    raw = read_csv(os.path.join(INPUT_DIR, "adrs_raw.csv"))
    adsl = read_csv(os.path.join(INPUT_DIR, "adsl.csv"))

    subjects = {}
    for row in adsl:
        subjects[(row["STUDYID"], row["USUBJID"])] = row

    out_rows = []
    for row in raw:
        key = (row["STUDYID"], row["USUBJID"])
        subject = subjects.get(key, {})
        saefl = subject.get("SAEFL", "") or ""
        dcsreas = subject.get("DCSREAS", "") or ""

        pchg_text = row["PCHG"]
        pchg = None
        if pchg_text is not None and pchg_text.strip() != "":
            pchg = float(pchg_text)

        if saefl == "Y" or dcsreas != "":
            avalc = "NON-RESPONDER"
            arsn = "SAFETY OR DISCONTINUATION RULE"
        elif pchg is None:
            avalc = "NOT EVALUABLE"
            arsn = "COMPONENT MISSING"
        elif pchg <= -75:
            avalc = "RESPONDER"
            arsn = "THRESHOLD MET"
        else:
            avalc = "NON-RESPONDER"
            arsn = "THRESHOLD NOT MET"

        if avalc == "RESPONDER":
            aval = "1"
        elif avalc == "NON-RESPONDER":
            aval = "0"
        else:
            aval = ""

        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": "RESP75",
                "PARAM": "EASI-75 Response",
                "AVISIT": row["AVISIT"],
                "PCHG": pchg_text,
                "SAEFL": saefl,
                "DCSREAS": dcsreas,
                "AVALC": avalc,
                "ARSN": arsn,
                "AVAL": aval,
            }
        )

    fieldnames = [
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "AVISIT",
        "PCHG",
        "SAEFL",
        "DCSREAS",
        "AVALC",
        "ARSN",
        "AVAL",
    ]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(
        os.path.join(OUTPUT_DIR, "adrs.csv"), "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
