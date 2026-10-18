# Reference solution for the yamaa benchmark adam-adtr-sum (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADTR: sum of target lesion diameters per subject and assessment.

Reads TR, TRVISIT and TU from /app/input and writes /app/output/adtr.csv
with one record per subject and scheduled tumor assessment (from TRVISIT).
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adtr.csv"

PARAMCD = "SDIAM"
PARAM = "Sum of Target Lesion Diameters (mm)"


def format_number(value):
    """Render a number without a trailing .0 when it is integral."""
    if pd.isna(value):
        return ""
    number = float(value)
    if number == int(number):
        return str(int(number))
    return repr(number)


def main():
    tr = pd.read_csv(f"{INPUT_DIR}/tr.csv", dtype=str, keep_default_na=False)
    trvisit = pd.read_csv(
        f"{INPUT_DIR}/trvisit.csv", dtype=str, keep_default_na=False
    )
    tu = pd.read_csv(f"{INPUT_DIR}/tu.csv", dtype=str, keep_default_na=False)

    # Numeric result; blank text becomes missing.
    tr["TRSTRESN_NUM"] = pd.to_numeric(tr["TRSTRESN"], errors="coerce")

    # Target longest-diameter records only.
    target = tr[(tr["TRGRPID"] == "TARGET") & (tr["TRTESTCD"] == "LDIAM")]

    # Number of target lesions selected at study entry, per subject.
    ntarget = (
        tu[tu["TUGRPID"] == "TARGET"]
        .groupby(["STUDYID", "USUBJID"])
        .size()
        .rename("NTARGET")
    )

    rows = []
    for _, visit in trvisit.iterrows():
        key = (visit["STUDYID"], visit["USUBJID"], visit["AVISIT"])
        records = target[
            (target["STUDYID"] == key[0])
            & (target["USUBJID"] == key[1])
            & (target["AVISIT"] == key[2])
        ]
        n_target = int(ntarget.get((key[0], key[1]), 0))

        if len(records) == 0:
            # No target longest-diameter records at this assessment.
            aval = ""
            nmeas = ""
        else:
            measured = records["TRSTRESN_NUM"].dropna()
            nmeas = str(len(measured))
            # A missing result contributes nothing; no measured lesion
            # means the sum has no value. A zero result counts as measured.
            aval = format_number(measured.sum()) if len(measured) > 0 else ""

        anl01fl = "Y" if nmeas != "" and int(nmeas) == n_target else ""

        rows.append(
            {
                "STUDYID": visit["STUDYID"],
                "USUBJID": visit["USUBJID"],
                "AVISIT": visit["AVISIT"],
                "AVISITN": format_number(
                    pd.to_numeric(visit["AVISITN"], errors="coerce")
                ),
                "ADT": visit["ADT"],
                "PARAMCD": PARAMCD,
                "PARAM": PARAM,
                "AVAL": aval,
                "NMEAS": nmeas,
                "NTARGET": str(n_target),
                "ANL01FL": anl01fl,
            }
        )

    columns = [
        "STUDYID",
        "USUBJID",
        "AVISIT",
        "AVISITN",
        "ADT",
        "PARAMCD",
        "PARAM",
        "AVAL",
        "NMEAS",
        "NTARGET",
        "ANL01FL",
    ]
    adtr = pd.DataFrame(rows, columns=columns)
    adtr["_VISITN_NUM"] = pd.to_numeric(adtr["AVISITN"], errors="coerce")
    adtr = adtr.sort_values(["USUBJID", "_VISITN_NUM"]).drop(
        columns="_VISITN_NUM"
    )
    adtr = adtr.reset_index(drop=True)
    adtr.to_csv(OUTPUT_PATH, index=False)
    print(adtr.to_string(index=False))


if __name__ == "__main__":
    main()
