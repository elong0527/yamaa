# Reference solution for the yamaa benchmark sdtm-lb-character-results (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from the ODM extract in /app/input.

One output record per laboratory result (per form occurrence). Writes
/app/output/lb.csv.
"""

import re

import pandas as pd

INPUT_ODM = "/app/input/odm.csv"
OUTPUT_LB = "/app/output/lb.csv"

# Collection-form order used for LBSEQ, with test codes and names.
FORM_ORDER = {
    "FO.LB_PROT": (0, "PROT", "Protein"),
    "FO.LB_CK": (1, "CK", "Creatine Kinase"),
    "FO.LB_GLUC": (2, "GLUC", "Glucose"),
    "FO.LB_KETON": (3, "KETON", "Ketones"),
    "FO.LB_CREAT": (4, "CREAT", "Creatinine"),
}

# Dipstick spellings (upper-cased) folded to standardized results.
DIPSTICK = {
    "NEGATIVE": "NEGATIVE",
    "NEG": "NEGATIVE",
    "TRACE": "TRACE",
    "TR": "TRACE",
    "1+": "1+",
    "+1": "1+",
    "1 PLUS": "1+",
    "2+": "2+",
    "+2": "2+",
}

PLAIN_NUMBER = re.compile(r"^[+-]?(\d+(\.\d*)?|\.\d+)$")

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "LBSEQ",
    "LBTESTCD",
    "LBTEST",
    "LBORRES",
    "LBORRESU",
    "LBSTRESC",
    "LBSTRESN",
    "LBSTRESU",
    "LBNRIND",
    "LBDTC",
]


def standardize(result):
    """Fold known dipstick spellings; keep anything else as collected."""
    return DIPSTICK.get(result.upper(), result)


def main():
    odm = pd.read_csv(INPUT_ODM, dtype=str, keep_default_na=False)

    # Pivot the long ODM extract to one row per form occurrence.
    key_cols = [
        "StudyOID",
        "SubjectKey",
        "StudyEventOID",
        "StudyEventRepeatKey",
        "FormOID",
        "FormRepeatKey",
    ]
    occurrences = {}
    order = []
    for row in odm.itertuples(index=False):
        key = tuple(getattr(row, col) for col in key_cols)
        if key not in occurrences:
            occurrences[key] = {}
            order.append(key)
        occurrences[key][row.ItemOID] = row.Value

    records = []
    for key in order:
        study_oid, subject_key, _event, _event_rep, form_oid, form_rep = key
        items = occurrences[key]
        if form_oid not in FORM_ORDER:
            continue
        form_rank, testcd, test = FORM_ORDER[form_oid]
        result = items.get("IT.LB.RESULT", "")
        unit = items.get("IT.LB.LBORRESU", "")
        stresc = standardize(result)
        if PLAIN_NUMBER.match(stresc):
            stresn = float(stresc)
        else:
            stresn = ""
        if stresc.startswith("<"):
            nrind = "LOW"
        elif stresc.startswith(">"):
            nrind = "HIGH"
        else:
            nrind = ""
        records.append(
            {
                "DOMAIN": "LB",
                "STUDYID": study_oid,
                "USUBJID": subject_key,
                "_form_rank": form_rank,
                "_form_rep": int(form_rep) if form_rep.isdigit() else 0,
                "LBTESTCD": testcd,
                "LBTEST": test,
                "LBORRES": result,
                "LBORRESU": unit,
                "LBSTRESC": stresc,
                "LBSTRESN": stresn,
                "LBSTRESU": unit,
                "LBNRIND": nrind,
                "LBDTC": items.get("IT.LB.LBDTC", ""),
            }
        )

    records.sort(key=lambda r: (r["USUBJID"], r["_form_rank"], r["_form_rep"]))

    seq = {}
    for record in records:
        subject = record["USUBJID"]
        seq[subject] = seq.get(subject, 0) + 1
        record["LBSEQ"] = seq[subject]
        del record["_form_rank"]
        del record["_form_rep"]

    lb = pd.DataFrame(records, columns=COLUMNS)
    lb.to_csv(OUTPUT_LB, index=False)


if __name__ == "__main__":
    main()
