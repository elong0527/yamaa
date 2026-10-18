# Reference solution for the yamaa benchmark adam-adsl-period-status (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

PERIODS = ["TREATMENT 1", "TREATMENT 2"]


def read_csv(name):
    with open(os.path.join(INPUT_DIR, name), newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def main():
    dm = read_csv("dm.csv")
    ds = read_csv("ds.csv")
    ex = read_csv("ex.csv")

    # Dosed periods per subject: any EX record in the period's epoch.
    dosed = {}
    for row in ex:
        dosed.setdefault(row["USUBJID"], set()).add(row["EPOCH"])

    # Disposition event per subject and period: DS record with category
    # "DISPOSITION EVENT" in the period's epoch; keep its coded term.
    event = {}
    for row in ds:
        if row["DSCAT"] == "DISPOSITION EVENT" and row["EPOCH"] in PERIODS:
            event[(row["USUBJID"], row["EPOCH"])] = row["DSDECOD"]

    rows = []
    for subj in dm:
        usubjid = subj["USUBJID"]
        out = {"STUDYID": subj["STUDYID"], "USUBJID": usubjid}
        for i, epoch in enumerate(PERIODS, start=1):
            status_col = f"EOP0{i}STT"
            reason_col = f"DCP0{i}RS"
            decod = event.get((usubjid, epoch))
            if decod is not None:
                if decod == "COMPLETED":
                    out[status_col] = "COMPLETED"
                    out[reason_col] = ""
                else:
                    out[status_col] = "DISCONTINUED"
                    out[reason_col] = decod
            elif epoch in dosed.get(usubjid, set()):
                out[status_col] = "ONGOING"
                out[reason_col] = ""
            else:
                out[status_col] = ""
                out[reason_col] = ""
        rows.append(out)

    fieldnames = ["STUDYID", "USUBJID", "EOP01STT", "DCP01RS", "EOP02STT", "DCP02RS"]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adsl.csv"), "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
