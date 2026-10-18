# Reference solution for the yamaa benchmark adam-adsl-crossover-sequence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADSL planned treatment sequence from DM, TA and TE.

One record per subject in DM. TRT01P/TRT02P are the names (from TE) of the
first and second treatment elements of the subject's planned arm (DM ARMCD),
taking the arm's elements from TA in TAETORD order and counting only
elements whose epoch name begins with "TREATMENT". TRTSEQP joins the two
with "/". Subjects with no planned arm code get no values.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(name):
    with open(os.path.join(INPUT_DIR, name), newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def main():
    dm = read_csv("dm.csv")
    ta = read_csv("ta.csv")
    te = read_csv("te.csv")

    # Element code -> element name, per study.
    element_name = {}
    for row in te:
        element_name[(row["STUDYID"], row["ETCD"])] = row["ELEMENT"]

    # Planned arm -> ordered treatment element codes, per study.
    arm_elements = {}
    for row in ta:
        if not row["EPOCH"].startswith("TREATMENT"):
            continue
        key = (row["STUDYID"], row["ARMCD"])
        arm_elements.setdefault(key, []).append(
            (int(row["TAETORD"]), row["ETCD"])
        )
    for key in arm_elements:
        arm_elements[key] = [
            etcd for _, etcd in sorted(arm_elements[key])
        ]

    out_rows = []
    for subj in dm:
        studyid = subj["STUDYID"]
        armcd = subj.get("ARMCD", "")
        trt01p = trt02p = trtseqp = ""
        if armcd:
            codes = arm_elements.get((studyid, armcd), [])
            names = [element_name.get((studyid, c), "") for c in codes]
            if len(names) >= 1:
                trt01p = names[0]
            if len(names) >= 2:
                trt02p = names[1]
            parts = [n for n in (trt01p, trt02p) if n]
            if parts:
                trtseqp = "/".join(parts)
        out_rows.append(
            {
                "STUDYID": studyid,
                "USUBJID": subj["USUBJID"],
                "TRT01P": trt01p,
                "TRT02P": trt02p,
                "TRTSEQP": trtseqp,
            }
        )

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "TRT01P", "TRT02P", "TRTSEQP"]
    with open(
        os.path.join(OUTPUT_DIR, "adsl.csv"), "w", newline="", encoding="utf-8"
    ) as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
