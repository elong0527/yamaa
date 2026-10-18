# Reference solution for the yamaa benchmark sdtm-dm-arms (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_rows(path):
    with open(path, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def clean(value):
    return (value or "").strip()


def main():
    rand_rows = read_rows(os.path.join(INPUT_DIR, "rand.csv"))
    ex_rows = read_rows(os.path.join(INPUT_DIR, "ex.csv"))

    # Planned arms as they appear in the randomization records, keyed by
    # upper-cased description and by upper-cased code.
    arms_by_desc = {}
    arms_by_code = {}
    for row in rand_rows:
        code = clean(row.get("RANDCD"))
        desc = clean(row.get("RAND"))
        if code and desc:
            arms_by_desc.setdefault(desc.upper(), (code, desc))
            arms_by_code.setdefault(code.upper(), (code, desc))

    # One record per subject: randomization order first, then any
    # exposure-only subjects in exposure order.
    subjects = []
    seen = set()
    rand_by_subject = {}
    for row in rand_rows:
        key = (clean(row.get("STUDYID")), clean(row.get("USUBJID")))
        if key not in seen:
            seen.add(key)
            subjects.append(key)
        rand_by_subject.setdefault(key, row)

    # Treatment received per subject (first exposure record).
    treatment = {}
    for row in ex_rows:
        key = (clean(row.get("STUDYID")), clean(row.get("USUBJID")))
        if key not in seen:
            seen.add(key)
            subjects.append(key)
        if key not in treatment:
            treatment[key] = clean(row.get("EXTRT"))

    out_rows = []
    for studyid, usubjid in subjects:
        key = (studyid, usubjid)
        rand = rand_by_subject.get(key, {})
        armcd = clean(rand.get("RANDCD"))
        arm = clean(rand.get("RAND")) if armcd else ""
        scrnfl = clean(rand.get("SCRNFL"))

        trt = treatment.get(key, "")
        actarmcd = ""
        actarm = ""
        actarmud = ""
        if trt:
            match = arms_by_desc.get(trt.upper()) or arms_by_code.get(
                trt.upper()
            )
            if match:
                actarmcd, actarm = match
            else:
                actarmud = trt

        armnrs = ""
        if not armcd and not actarmcd and not trt:
            armnrs = "SCREEN FAILURE" if scrnfl == "Y" else "NOT ASSIGNED"
        elif armcd and not trt:
            armnrs = "NOT TREATED"

        out_rows.append(
            {
                "DOMAIN": "DM",
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "ARMCD": armcd,
                "ARM": arm,
                "ACTARMCD": actarmcd,
                "ACTARM": actarm,
                "ARMNRS": armnrs,
                "ACTARMUD": actarmud,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "ARMCD",
        "ARM",
        "ACTARMCD",
        "ACTARM",
        "ARMNRS",
        "ACTARMUD",
    ]
    with open(
        os.path.join(OUTPUT_DIR, "dm.csv"), "w", newline="", encoding="utf-8"
    ) as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
