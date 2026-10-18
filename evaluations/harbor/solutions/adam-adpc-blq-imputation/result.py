# Reference solution for the yamaa benchmark adam-adpc-blq-imputation (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADPC from PC with BLQ imputation.

One record per sample. AVAL is the quantified concentration as recorded;
a below-limit result ("<...") is 0 before the first or after the last
quantified result of its profile (subject, analyte, dose), and blank
between them. A not-done sample has no AVAL and neither starts nor ends
the quantified span.
"""

import csv
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")


def main() -> None:
    with (INPUT / "pc.csv").open(newline="") as f:
        rows = list(csv.DictReader(f))

    # Classify each sample.
    for r in rows:
        stat = (r.get("PCSTAT") or "").strip()
        stresc = (r.get("PCSTRESC") or "").strip()
        if stat == "NOT DONE" or stresc == "":
            r["_kind"] = "notdone"
        elif stresc.startswith("<"):
            r["_kind"] = "blq"
        else:
            r["_kind"] = "quant"

    # Profiles: one subject, one analyte, one dose, in planned time order.
    profiles = {}
    for r in rows:
        key = (r["USUBJID"], r["PCTESTCD"], r["PCTPTREF"])
        profiles.setdefault(key, []).append(r)
    for members in profiles.values():
        members.sort(key=lambda r: (float(r["PCTPTNUM"]), int(r["PCSEQ"])))
        quant_pos = [i for i, r in enumerate(members) if r["_kind"] == "quant"]
        for i, r in enumerate(members):
            if r["_kind"] == "quant":
                r["_aval"] = (r.get("PCSTRESN") or "").strip()
            elif r["_kind"] == "blq":
                if not quant_pos or i < quant_pos[0] or i > quant_pos[-1]:
                    r["_aval"] = "0"
                else:
                    r["_aval"] = ""
            else:
                r["_aval"] = ""

    out_fields = [
        "STUDYID", "USUBJID", "PCSEQ", "PARAMCD", "ATPTREF",
        "ATPT", "ATPTN", "AVAL", "AVALU",
    ]
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with (OUTPUT / "adpc.csv").open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=out_fields)
        w.writeheader()
        for r in rows:
            w.writerow({
                "STUDYID": r["STUDYID"],
                "USUBJID": r["USUBJID"],
                "PCSEQ": r["PCSEQ"],
                "PARAMCD": r["PCTESTCD"],
                "ATPTREF": r["PCTPTREF"],
                "ATPT": r["PCTPT"],
                "ATPTN": r["PCTPTNUM"],
                "AVAL": r["_aval"],
                "AVALU": "ng/mL",
            })


if __name__ == "__main__":
    main()
