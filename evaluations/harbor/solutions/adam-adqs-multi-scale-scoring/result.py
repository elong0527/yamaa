# Reference solution for the yamaa benchmark adam-adqs-multi-scale-scoring (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT = "/app/input"
OUTPUT = "/app/output"

# scale: (score PARAMCD, PARAM, items, anchor, min answered, rescale fn)
SCALES = [
    ("F1SCORE", "Physical Functioning Scale Score",
     ["F101", "F102", "F103", "F104"], "F101", 2,
     lambda m: (4.0 - m) / 3.0 * 100.0),
    ("F2SCORE", "Role Functioning Scale Score",
     ["F201", "F202"], "F201", 1,
     lambda m: (4.0 - m) / 3.0 * 100.0),
    ("SSCORE", "Fatigue and Sleep Symptom Scale Score",
     ["S01", "S02", "F104"], "S01", 2,
     lambda m: (m - 1.0) / 3.0 * 100.0),
    ("GSCORE", "Global Health Scale Score",
     ["G01", "G02"], "G01", 1,
     lambda m: (m - 1.0) / 6.0 * 100.0),
]


def fmt(value):
    if value is None:
        return ""
    if float(value).is_integer():
        return str(int(value))
    return repr(float(value))


def main():
    with open(f"{INPUT}/qs.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    groups = OrderedDict()  # (study, subj, visit) -> list of rows
    for r in rows:
        key = (r["STUDYID"], r["USUBJID"], r["VISIT"])
        groups.setdefault(key, []).append(r)

    out = []
    for (study, subj, visit), grows in groups.items():
        for r in grows:
            aval = r["QSSTRESN"].strip()
            out.append([study, subj, visit, r["QSTESTCD"], r["QSTEST"],
                        aval if aval else ""])
        by_code = {}
        for r in grows:
            by_code.setdefault(r["QSTESTCD"], []).append(r)
        for pcd, pname, items, anchor, min_n, scale_fn in SCALES:
            if anchor not in by_code:
                continue
            answered = []
            for code in items:
                for r in by_code.get(code, []):
                    v = r["QSSTRESN"].strip()
                    if v:
                        answered.append(float(v))
            if len(answered) >= min_n:
                aval = fmt(scale_fn(sum(answered) / len(answered)))
            else:
                aval = ""
            out.append([study, subj, visit, pcd, pname, aval])

    with open(f"{OUTPUT}/adqs.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL"])
        w.writerows(out)


if __name__ == "__main__":
    main()
