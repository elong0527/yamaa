# Reference solution for the yamaa benchmark adam-adae-severity (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADAE dataset from the AE and SUPP datasets.

One record per adverse event in AE. AESEV is the severity recorded in
SUPP, matched to an event on STUDYID, USUBJID, and AESEQ together.
AESEV has no value when the event has no supplemental record, or when
its supplemental record carries no severity. A supplemental record for
an event that was never collected creates no row.
"""

import pandas as pd

AE = pd.read_csv("/app/input/ae.csv", dtype=str, keep_default_na=False)
SUPP = pd.read_csv("/app/input/supp.csv", dtype=str, keep_default_na=False)

KEYS = ["STUDYID", "USUBJID", "AESEQ"]

adae = AE.merge(SUPP[KEYS + ["AESEV"]], on=KEYS, how="left", sort=False)
adae["AESEV"] = adae["AESEV"].fillna("")

adae = adae[["STUDYID", "USUBJID", "AESEQ", "AEDECOD", "AESEV"]]
adae.to_csv("/app/output/adae.csv", index=False)
