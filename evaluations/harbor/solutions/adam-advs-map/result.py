# Reference solution for the yamaa benchmark adam-advs-map (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT_PATH = "/app/input/advs.csv"
OUTPUT_PATH = "/app/output/advs.csv"

COLUMNS = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "VISIT", "DTYPE"]


def main():
    with open(INPUT_PATH, newline="") as f:
        rows = list(csv.DictReader(f))

    # Group collected records by (subject, visit), preserving input order.
    groups = OrderedDict()
    for row in rows:
        key = (row["USUBJID"], row["VISIT"])
        groups.setdefault(key, []).append(row)

    out_rows = []
    for (usubjid, visit), group in groups.items():
        for row in group:
            out_rows.append({
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "PARAMCD": row["PARAMCD"],
                "PARAM": row["PARAM"],
                "AVAL": row["AVAL"],
                "VISIT": row["VISIT"],
                "DTYPE": "",
            })

        values = {}
        for row in group:
            if row["PARAMCD"] in ("SYSBP", "DIABP"):
                aval = row["AVAL"]
                if aval is not None and aval.strip() != "":
                    values[row["PARAMCD"]] = float(aval)

        if "SYSBP" in values and "DIABP" in values:
            map_aval = (2.0 / 3.0) * values["DIABP"] + (1.0 / 3.0) * values["SYSBP"]
            out_rows.append({
                "STUDYID": group[0]["STUDYID"],
                "USUBJID": usubjid,
                "PARAMCD": "MAP",
                "PARAM": "Mean Arterial Pressure (mmHg)",
                "AVAL": repr(map_aval),
                "VISIT": visit,
                "DTYPE": "CALCULATION",
            })

    with open(OUTPUT_PATH, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
