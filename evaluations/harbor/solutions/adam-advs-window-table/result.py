# Reference solution for the yamaa benchmark adam-advs-window-table (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def to_int(text):
    if text is None or text.strip() == "":
        return None
    return int(text)


def main():
    raw = read_csv(f"{INPUT_DIR}/advs_raw.csv")
    windows = read_csv(f"{INPUT_DIR}/awindow.csv")

    windows_by_study = {}
    for w in windows:
        windows_by_study.setdefault(w["STUDYID"], []).append(w)

    records = []
    for row in raw:
        ady = to_int(row["ADY"])
        matched = None
        if ady is not None:
            for w in windows_by_study.get(row["STUDYID"], []):
                lo = to_int(w["AWLO"])
                hi = to_int(w["AWHI"])
                # An absent bound is not an open-ended one: a window
                # whose last day was never stated matches nothing.
                if lo is None or hi is None:
                    continue
                if lo <= ady <= hi:
                    matched = w
                    break

        rec = {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "PARAMCD": row["PARAMCD"],
            "VSSEQ": row["VSSEQ"],
            "VISIT": row["VISIT"],
            "ADT": row["ADT"],
            "ADY": row["ADY"],
            "AVAL": row["AVAL"],
            "AVISIT": "",
            "AVISITN": "",
            "AWTARGET": "",
            "AWTDIFF": "",
            "ANL01FL": "",
        }
        diff = None
        if matched is not None:
            target = to_int(matched["AWTARGET"])
            rec["AVISIT"] = matched["AVISIT"]
            rec["AVISITN"] = matched["AVISITN"]
            rec["AWTARGET"] = matched["AWTARGET"]
            if target is not None:
                diff = ady - target
                rec["AWTDIFF"] = str(diff)
        rec["_diff"] = diff
        records.append(rec)

    # ANL01FL is Y on the record nearest its window target among the
    # records for the same study, subject, parameter and analysis
    # visit; the lower VSSEQ breaks a tie.
    groups = {}
    for rec in records:
        if rec["AVISIT"] == "" or rec["_diff"] is None:
            continue
        key = (rec["STUDYID"], rec["USUBJID"], rec["PARAMCD"], rec["AVISIT"])
        groups.setdefault(key, []).append(rec)
    for group in groups.values():
        best = min(group, key=lambda r: (abs(r["_diff"]), int(r["VSSEQ"])))
        best["ANL01FL"] = "Y"

    fieldnames = [
        "STUDYID", "USUBJID", "PARAMCD", "VSSEQ", "VISIT", "ADT", "ADY",
        "AVAL", "AVISIT", "AVISITN", "AWTARGET", "AWTDIFF", "ANL01FL",
    ]
    with open(f"{OUTPUT_DIR}/advs.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for rec in records:
            writer.writerow({k: rec[k] for k in fieldnames})


if __name__ == "__main__":
    main()
