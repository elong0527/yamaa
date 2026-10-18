# Reference solution for the yamaa benchmark adam-adfa-fever (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADFA (Fever Occurrence) from the VS dataset.

One record per qualifying temperature record: VSTESTCD "TEMP" in the
"REACTOGENICITY" category. AVALC is "Y" for a temperature of 38 C or
higher and "N" below that; it has no value when the result is missing
or the unit is not "C". AVAL is 1 for "Y" and 0 for "N".
"""

import csv

INPUT_VS = "/app/input/vs.csv"
OUTPUT_ADFA = "/app/output/adfa.csv"

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "ASEQ", "PARAMCD", "PARAM",
    "AVAL", "AVALC", "ADT", "SRCDOM", "SRCVAR", "SRCSEQ",
]


def main():
    with open(INPUT_VS, newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    qualifying = [
        r for r in rows
        if r["VSTESTCD"] == "TEMP" and r["VSCAT"] == "REACTOGENICITY"
    ]

    # Group by subject, keeping first-appearance order of subjects.
    subjects = []
    by_subject = {}
    for r in qualifying:
        key = (r["STUDYID"], r["USUBJID"])
        if key not in by_subject:
            by_subject[key] = []
            subjects.append(key)
        by_subject[key].append(r)

    out = []
    for studyid, usubjid in subjects:
        recs = by_subject[(studyid, usubjid)]
        recs.sort(key=lambda r: (r["VSDTC"][:10], float(r["VSSEQ"])))
        for i, r in enumerate(recs, start=1):
            result = r["VSSTRESN"].strip()
            unit = r["VSSTRESU"].strip()
            if result == "" or unit != "C":
                avalc = ""
                aval = ""
            elif float(result) >= 38:
                avalc = "Y"
                aval = "1"
            else:
                avalc = "N"
                aval = "0"
            seq = r["VSSEQ"].strip()
            srcseq = str(int(float(seq))) if seq else ""
            out.append({
                "DOMAIN": "ADFA",
                "STUDYID": studyid,
                "USUBJID": usubjid,
                "ASEQ": str(i),
                "PARAMCD": "FEVER",
                "PARAM": "Fever Occurrence",
                "AVAL": aval,
                "AVALC": avalc,
                "ADT": r["VSDTC"][:10],
                "SRCDOM": "VS",
                "SRCVAR": "VSSTRESN",
                "SRCSEQ": srcseq,
            })

    with open(OUTPUT_ADFA, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out)


if __name__ == "__main__":
    main()
