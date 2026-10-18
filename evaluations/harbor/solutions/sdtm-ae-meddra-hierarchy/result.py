# Reference solution for the yamaa benchmark sdtm-ae-meddra-hierarchy (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

ae = pd.read_csv(f"{INPUT_DIR}/ae_raw.csv", dtype=str, keep_default_na=False)
meddra = pd.read_csv(
    f"{INPUT_DIR}/meddra_synthetic.csv", dtype=str, keep_default_na=False
)

# Keep only the primary path for each LLT code.
primary = meddra[meddra["PRIMARY_SOC"] == "Y"]
hierarchy = {}
for _, row in primary.iterrows():
    hierarchy[row["LLTCD"]] = row

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "AESEQ", "AETERM", "AELLT", "AELLTCD",
    "AEDECOD", "AEPTCD", "AEHLT", "AEHLTCD", "AEHLGT", "AEHLGTCD",
    "AEBODSYS", "AEBODSCD", "AESOC", "AESOCCD",
]

records = []
for _, event in ae.iterrows():
    code = event["AELLTCD"]
    record = {
        "DOMAIN": "AE",
        "STUDYID": event["STUDYID"],
        "USUBJID": event["USUBJID"],
        "AESEQ": event["AESEQ"],
        "AETERM": event["AETERM"],
        "AELLT": "",
        "AELLTCD": code,
        "AEDECOD": "",
        "AEPTCD": "",
        "AEHLT": "",
        "AEHLTCD": "",
        "AEHLGT": "",
        "AEHLGTCD": "",
        "AEBODSYS": "",
        "AEBODSCD": "",
        "AESOC": "",
        "AESOCCD": "",
    }
    if code and code in hierarchy:
        path = hierarchy[code]
        record["AELLT"] = path["LLTNAME"]
        record["AEDECOD"] = path["PTNAME"]
        record["AEPTCD"] = path["PTCD"]
        record["AEHLT"] = path["HLTNAME"]
        record["AEHLTCD"] = path["HLTCD"]
        record["AEHLGT"] = path["HLGTNAME"]
        record["AEHLGTCD"] = path["HLGTCD"]
        record["AEBODSYS"] = path["SOCNAME"]
        record["AEBODSCD"] = path["SOCCD"]
        record["AESOC"] = path["SOCNAME"]
        record["AESOCCD"] = path["SOCCD"]
    records.append(record)

out = pd.DataFrame(records, columns=COLUMNS)
out.to_csv(f"{OUTPUT_DIR}/ae.csv", index=False)
