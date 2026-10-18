# Reference solution for the yamaa benchmark sdtm-cm-whodrug-coding (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM CM dataset from ODM, CODING_OUTPUT and WHODRUG_EXTRACT.

One output record per reported medication (the IT.CM.CMTRT item in the
ODM long-format extract).  Each reported medication is joined to the
coder's output on STUDYID, USUBJID and CMSEQ to obtain the chosen drug
record and ATC code, and then to the WHODrug extract on that drug
record together with that ATC code to obtain the preferred name and
the ATC class for the assigned use.  Medications with no coding row
are not yet coded, so CMDECOD, CMCLAS and CMCLASCD stay blank.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_FILE = "/app/output/cm.csv"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "CMSEQ",
    "CMTRT",
    "CMDECOD",
    "CMCLAS",
    "CMCLASCD",
]


def read_csv(name):
    return pd.read_csv(
        f"{INPUT_DIR}/{name}", dtype=str, keep_default_na=False
    )


def main():
    odm = read_csv("odm.csv")
    coding = read_csv("coding_output.csv")
    whodrug = read_csv("whodrug_extract.csv")

    # One record per reported medication: the CMTRT item of each IG.CM
    # item-group repeat.  The repeat key is the CM sequence number and
    # the reported name is kept exactly as written.
    reported = odm.loc[odm["ItemOID"] == "IT.CM.CMTRT"].copy()
    cm = pd.DataFrame(
        {
            "DOMAIN": "CM",
            "STUDYID": reported["StudyOID"],
            "USUBJID": reported["SubjectKey"],
            "CMSEQ": reported["ItemGroupRepeatKey"],
            "CMTRT": reported["Value"],
        }
    )

    # The drug record and ATC code the coder assigned to each record.
    cm = cm.merge(
        coding[["STUDYID", "USUBJID", "CMSEQ", "DRUG_RECORD_NO", "ATC_CODE"]],
        on=["STUDYID", "USUBJID", "CMSEQ"],
        how="left",
    )

    # Preferred name and ATC class for the chosen drug record under the
    # assigned ATC code (the same drug record can carry a different
    # class under a different ATC code).
    cm = cm.merge(
        whodrug[
            [
                "DRUG_RECORD_NO",
                "ATC_CODE",
                "PREFERRED_NAME",
                "ATC_CLASS_NAME",
                "ATC_CLASS_CODE",
            ]
        ],
        on=["DRUG_RECORD_NO", "ATC_CODE"],
        how="left",
    )

    cm = cm.rename(
        columns={
            "PREFERRED_NAME": "CMDECOD",
            "ATC_CLASS_NAME": "CMCLAS",
            "ATC_CLASS_CODE": "CMCLASCD",
        }
    )
    cm = cm[COLUMNS].fillna("")
    cm.to_csv(OUTPUT_FILE, index=False)


if __name__ == "__main__":
    main()
