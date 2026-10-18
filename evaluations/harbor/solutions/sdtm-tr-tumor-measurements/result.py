# Reference solution for the yamaa benchmark sdtm-tr-tumor-measurements (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM TR dataset from TR_RAW.

One record per collected tumor assessment. Diameter results are
standardized to mm: a value collected in mm keeps its value, a value
collected in cm is multiplied by 10, and a lesion too small to measure
takes 5. Lesion states are kept as collected. A lesion that was not
assessed carries TRSTAT "NOT DONE" and no standardized result.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

TOO_SMALL = "TOO SMALL TO MEASURE"
TOO_SMALL_MM = 5.0

OUT_COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "TRSEQ", "TRLNKID", "TRTESTCD",
    "TRTEST", "TRORRES", "TRORRESU", "TRSTRESC", "TRSTRESN", "TRSTRESU",
    "TRSTAT", "TRMETHOD", "TREVAL", "VISITNUM", "TRDTC",
]


def fmt_number(value):
    """Write a float without a trailing .0 or float artifacts."""
    if value == int(value):
        return str(int(value))
    return repr(value)


def main():
    with open(os.path.join(INPUT_DIR, "tr_raw.csv"), newline="") as fh:
        rows = list(csv.DictReader(fh))

    out_rows = []
    for row in rows:
        testcd = row["TRTESTCD"]
        orres = row["TRORRES"]
        orresu = row["TRORRESU"]
        stat = row["TRSTAT"]

        stresn = ""
        stresc = ""
        if testcd == "DIAMETER":
            if orres == TOO_SMALL:
                stresn = fmt_number(TOO_SMALL_MM)
                stresc = stresn
            elif orres != "":
                value = float(orres)
                if orresu == "cm":
                    value = value * 10
                stresn = fmt_number(value)
                stresc = stresn
        elif testcd == "TUMSTATE":
            stresc = orres

        stresu = "mm" if stresn != "" else ""

        out_rows.append({
            "DOMAIN": "TR",
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "TRSEQ": row["TRSEQ"],
            "TRLNKID": row["TRLNKID"],
            "TRTESTCD": testcd,
            "TRTEST": row["TRTEST"],
            "TRORRES": orres,
            "TRORRESU": orresu,
            "TRSTRESC": stresc,
            "TRSTRESN": stresn,
            "TRSTRESU": stresu,
            "TRSTAT": stat,
            "TRMETHOD": row["TRMETHOD"],
            "TREVAL": row["TREVAL"],
            "VISITNUM": row["VISITNUM"],
            "TRDTC": row["TRDTC"],
        })

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "tr.csv"), "w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=OUT_COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
