# Reference solution for the yamaa benchmark sdtm-lb-character-results (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from the ODM extract in /app/input.

One record per laboratory result (per form occurrence). Character
(dipstick) results are folded to their standardized spellings, numeric
results carry LBSTRESN, and censored results (< / >) drive LBNRIND.
"""

import re

import polars as pl

INPUT = "/app/input"
OUTPUT = "/app/output"

# Collection order of the tests, with their short and long names.
TEST_ORDER = {
    "FO.LB_PROT": (1, "PROT", "Protein"),
    "FO.LB_CK": (2, "CK", "Creatine Kinase"),
    "FO.LB_GLUC": (3, "GLUC", "Glucose"),
    "FO.LB_KETON": (4, "KETON", "Ketones"),
    "FO.LB_CREAT": (5, "CREAT", "Creatinine"),
}

# Known dipstick spellings (matched case-insensitively) and the
# standardized result each folds to.
DIPSTICK_FOLDS = {
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

PLAIN_NUMBER = re.compile(r"^[+-]?\d+(\.\d+)?$")

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


def standardize(collected):
    """Fold known dipstick spellings; keep anything else as collected."""
    return DIPSTICK_FOLDS.get(collected.strip().upper(), collected)


def main():
    odm = pl.read_csv(f"{INPUT}/odm.csv", infer_schema_length=0).fill_null("")

    # Gather the items of each form occurrence.
    occurrences = {}
    subject_order = []
    for row in odm.iter_rows(named=True):
        key = (row["SubjectKey"], row["FormOID"], row["FormRepeatKey"])
        if row["SubjectKey"] not in subject_order:
            subject_order.append(row["SubjectKey"])
        occ = occurrences.setdefault(
            key,
            {
                "STUDYID": row["StudyOID"],
                "USUBJID": row["SubjectKey"],
                "FormOID": row["FormOID"],
                "FormRepeatKey": row["FormRepeatKey"],
                "LBORRES": "",
                "LBORRESU": "",
                "LBDTC": "",
            },
        )
        if row["ItemOID"] == "IT.LB.RESULT":
            occ["LBORRES"] = row["Value"]
        elif row["ItemOID"] == "IT.LB.LBDTC":
            occ["LBDTC"] = row["Value"]
        elif row["ItemOID"] == "IT.LB.LBORRESU":
            occ["LBORRESU"] = row["Value"]

    # Order records: subject (first appearance), then collection order
    # of the test, then form repeat.
    records = sorted(
        occurrences.values(),
        key=lambda o: (
            subject_order.index(o["USUBJID"]),
            TEST_ORDER[o["FormOID"]][0],
            int(o["FormRepeatKey"]),
        ),
    )

    out = {name: [] for name in COLUMNS}
    seq_by_subject = {}
    for occ in records:
        _, testcd, test = TEST_ORDER[occ["FormOID"]]
        orres = occ["LBORRES"]
        unit = occ["LBORRESU"]
        stresc = standardize(orres)
        stresn = stresc if PLAIN_NUMBER.match(stresc.strip()) else ""
        if stresc.startswith("<"):
            nrind = "LOW"
        elif stresc.startswith(">"):
            nrind = "HIGH"
        else:
            nrind = ""
        seq_by_subject[occ["USUBJID"]] = seq_by_subject.get(occ["USUBJID"], 0) + 1
        values = {
            "DOMAIN": "LB",
            "STUDYID": occ["STUDYID"],
            "USUBJID": occ["USUBJID"],
            "LBSEQ": seq_by_subject[occ["USUBJID"]],
            "LBTESTCD": testcd,
            "LBTEST": test,
            "LBORRES": orres,
            "LBORRESU": unit,
            "LBSTRESC": stresc,
            "LBSTRESN": stresn,
            "LBSTRESU": unit,
            "LBNRIND": nrind,
            "LBDTC": occ["LBDTC"],
        }
        for name in COLUMNS:
            out[name].append(values[name])

    # Blank values go in as nulls so they are written as empty fields.
    for name in COLUMNS:
        if name != "LBSEQ":
            out[name] = [v if v != "" else None for v in out[name]]
    lb = pl.DataFrame(out).select(COLUMNS)
    lb.write_csv(f"{OUTPUT}/lb.csv")


if __name__ == "__main__":
    main()
