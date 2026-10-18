# Reference solution for the yamaa benchmark adam-advs-bsa (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import math

import pandas as pd

INPUT = "/app/input"
OUTPUT = "/app/output"

COLUMNS = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVAL", "VISIT", "DTYPE"]


def main():
    advs = pd.read_csv(f"{INPUT}/advs.csv", dtype={"AVAL": float})

    # Collect non-missing results per subject and visit.
    results = {}
    for _, row in advs.iterrows():
        key = (row["STUDYID"], row["USUBJID"], row["VISIT"])
        results.setdefault(key, {})
        if not pd.isna(row["AVAL"]):
            results[key][row["PARAMCD"]] = row["AVAL"]

    records = []
    emitted = set()
    rows = list(advs.iterrows())
    for i, (_, row) in enumerate(rows):
        rec = {col: row[col] for col in COLUMNS if col != "DTYPE"}
        rec["DTYPE"] = ""
        records.append(rec)

        key = (row["STUDYID"], row["USUBJID"], row["VISIT"])
        # Emit the BSA record after the last collected record of its group.
        last_of_group = i == len(rows) - 1 or (
            rows[i + 1][1]["STUDYID"],
            rows[i + 1][1]["USUBJID"],
            rows[i + 1][1]["VISIT"],
        ) != key
        if last_of_group and key not in emitted:
            emitted.add(key)
            vals = results[key]
            if "HEIGHT" in vals and "WEIGHT" in vals:
                bsa = math.sqrt(vals["HEIGHT"] * vals["WEIGHT"] / 3600.0)
                records.append(
                    {
                        "STUDYID": key[0],
                        "USUBJID": key[1],
                        "PARAMCD": "BSA",
                        "PARAM": "Body Surface Area (m^2)",
                        "AVAL": bsa,
                        "VISIT": key[2],
                        "DTYPE": "CALCULATION",
                    }
                )

    out = pd.DataFrame(records, columns=COLUMNS)
    out.to_csv(f"{OUTPUT}/advs.csv", index=False)


if __name__ == "__main__":
    main()
