# Reference solution for the yamaa benchmark adam-adlb-shift-criteria (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def to_float(value):
    if value is None:
        return None
    value = value.strip()
    if value == "":
        return None
    return float(value)


def fmt_number(x):
    if x == int(x):
        return str(int(x))
    return repr(x)


def main():
    with open(os.path.join(INPUT_DIR, "adlb_raw.csv"), newline="") as f:
        rows = list(csv.DictReader(f))

    # Per-record ANRIND
    for row in rows:
        aval = to_float(row["AVAL"])
        anrlo = to_float(row["ANRLO"])
        anrhi = to_float(row["ANRHI"])
        if aval is None or anrlo is None or anrhi is None:
            row["_ANRIND"] = ""
        elif aval < anrlo:
            row["_ANRIND"] = "LOW"
        elif aval > anrhi:
            row["_ANRIND"] = "HIGH"
        else:
            row["_ANRIND"] = "NORMAL"

    # Baseline values per subject and parameter
    baselines = {}
    for row in rows:
        if row["ABLFL"].strip() == "Y":
            baselines[(row["USUBJID"], row["PARAMCD"])] = row

    out_rows = []
    for row in rows:
        aval = to_float(row["AVAL"])
        anrlo = to_float(row["ANRLO"])
        anrhi = to_float(row["ANRHI"])
        anrind = row["_ANRIND"]

        base_row = baselines.get((row["USUBJID"], row["PARAMCD"]))
        if base_row is None:
            base_txt = ""
            base_val = None
            bnrind = ""
        else:
            base_txt = base_row["AVAL"]
            base_val = to_float(base_row["AVAL"])
            bnrind = base_row["_ANRIND"]

        if anrind == "" or bnrind == "":
            shift1 = ""
        else:
            shift1 = bnrind + " to " + anrind

        if aval is None or base_val is None or base_val == 0:
            r2base = ""
        else:
            r2base = fmt_number(aval / base_val)

        if aval is not None and anrhi is not None:
            crit1 = "Result greater than 3 x ULN"
            crit1fl = "Y" if aval > 3 * anrhi else "N"
        else:
            crit1 = ""
            crit1fl = ""

        if aval is not None and anrlo is not None:
            crit2 = "Result less than LLN"
            crit2fl = "Y" if aval < anrlo else "N"
        else:
            crit2 = ""
            crit2fl = ""

        out_rows.append({
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "PARAMCD": row["PARAMCD"],
            "PARAM": row["PARAM"],
            "ASEQ": row["ASEQ"],
            "AVISIT": row["AVISIT"],
            "AVAL": row["AVAL"],
            "ANRLO": row["ANRLO"],
            "ANRHI": row["ANRHI"],
            "ANRIND": anrind,
            "ABLFL": row["ABLFL"],
            "BASE": base_txt,
            "BNRIND": bnrind,
            "SHIFT1": shift1,
            "R2BASE": r2base,
            "CRIT1": crit1,
            "CRIT1FL": crit1fl,
            "CRIT2": crit2,
            "CRIT2FL": crit2fl,
        })

    fieldnames = [
        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "ASEQ", "AVISIT",
        "AVAL", "ANRLO", "ANRHI", "ANRIND", "ABLFL", "BASE", "BNRIND",
        "SHIFT1", "R2BASE", "CRIT1", "CRIT1FL", "CRIT2", "CRIT2FL",
    ]
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adlb.csv"), "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
