# Reference solution for the yamaa benchmark adam-advs-window-table (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADVS from ADVS_RAW and AWINDOW.

Each measurement record is matched to the analysis window of its own
study whose [AWLO, AWHI] range (both ends inclusive) contains the
record's study day (ADY). A window whose last day was never stated is
not open-ended: a study day on or past its first day has no window.
ANL01FL flags, within each study/subject/parameter/analysis-visit
group, the record nearest the window target day (lowest VSSEQ wins a
tie).
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

OUT_COLUMNS = [
    "STUDYID", "USUBJID", "PARAMCD", "VSSEQ", "VISIT", "ADT", "ADY",
    "AVAL", "AVISIT", "AVISITN", "AWTARGET", "AWTDIFF", "ANL01FL",
]


def to_int(value):
    """Parse an integer cell; return None when the cell is blank."""
    if value is None:
        return None
    value = value.strip()
    if value == "":
        return None
    return int(value)


def read_csv(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def main():
    raw = read_csv(os.path.join(INPUT_DIR, "advs_raw.csv"))
    window_rows = read_csv(os.path.join(INPUT_DIR, "awindow.csv"))

    # Windows keyed by study. A window matches only when both bounds
    # were stated; an absent bound is not an open-ended one.
    windows = {}
    for row in window_rows:
        windows.setdefault(row["STUDYID"], []).append(
            {
                "avisit": row["AVISIT"],
                "avisitn": to_int(row["AVISITN"]),
                "awlo": to_int(row["AWLO"]),
                "awhi": to_int(row["AWHI"]),
                "awtarget": to_int(row["AWTARGET"]),
            }
        )

    records = []
    for row in raw:
        ady = to_int(row["ADY"])
        match = None
        if ady is not None:
            for window in windows.get(row["STUDYID"], []):
                if (
                    window["awlo"] is not None
                    and window["awhi"] is not None
                    and window["awlo"] <= ady <= window["awhi"]
                ):
                    match = window
                    break
        record = {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "PARAMCD": row["PARAMCD"],
            "VSSEQ": to_int(row["VSSEQ"]),
            "VISIT": row["VISIT"],
            "ADT": row["ADT"],
            "ADY": ady,
            "AVAL": row["AVAL"],
            "AVISIT": match["avisit"] if match else None,
            "AVISITN": match["avisitn"] if match else None,
            "AWTARGET": match["awtarget"] if match else None,
            "AWTDIFF": (ady - match["awtarget"]) if match else None,
            "ANL01FL": "",
        }
        records.append(record)

    # Flag the record nearest its window target within each
    # study/subject/parameter/analysis-visit group; the lower VSSEQ
    # breaks a tie. Records with no window are never flagged.
    groups = {}
    for record in records:
        if record["AVISIT"] is None:
            continue
        key = (
            record["STUDYID"],
            record["USUBJID"],
            record["PARAMCD"],
            record["AVISIT"],
        )
        groups.setdefault(key, []).append(record)
    for group in groups.values():
        best = min(
            group,
            key=lambda r: (abs(r["AWTDIFF"]), r["VSSEQ"]),
        )
        best["ANL01FL"] = "Y"

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    out_path = os.path.join(OUTPUT_DIR, "advs.csv")
    with open(out_path, "w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(OUT_COLUMNS)
        for record in records:
            writer.writerow(
                [
                    "" if record[col] is None else record[col]
                    for col in OUT_COLUMNS
                ]
            )


if __name__ == "__main__":
    main()
