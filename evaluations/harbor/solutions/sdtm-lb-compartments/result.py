# Reference solution for the yamaa benchmark sdtm-lb-compartments (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

bx = pl.read_csv("/app/input/bx_raw.csv", infer_schema=False)

rows = []
for r in bx.to_dicts():
    study, subj = r["STUDYID"], r["USUBJID"]
    cohort, les, nles, unit = r["COHORT"], r["LESRES"], r["NLESRES"], r["RESU"]
    records = []
    if cohort is not None and cohort != "" and cohort != "NONAD":
        records.append(("LESIONAL", les))
    records.append(("NON-LESIONAL", nles))
    for i, (loc, res) in enumerate(records, start=1):
        res_text = res if res not in (None, "") else None
        try:
            resn = float(res_text) if res_text is not None else None
        except ValueError:
            resn = None
        rows.append(
            {
                "DOMAIN": "LB",
                "STUDYID": study,
                "USUBJID": subj,
                "LBSEQ": i,
                "LBTESTCD": "IL13",
                "LBTEST": "Interleukin 13",
                "LBSPEC": "SKIN",
                "LBLOC": loc,
                "LBORRES": res_text,
                "LBORRESU": unit,
                "LBSTRESN": resn,
                "LBSTAT": "NOT DONE" if res_text is None else None,
            }
        )

lb = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "LBSEQ": pl.Int64,
        "LBTESTCD": pl.String,
        "LBTEST": pl.String,
        "LBSPEC": pl.String,
        "LBLOC": pl.String,
        "LBORRES": pl.String,
        "LBORRESU": pl.String,
        "LBSTRESN": pl.Float64,
        "LBSTAT": pl.String,
    },
).sort(["STUDYID", "USUBJID", "LBSEQ"])

Path("/app/output").mkdir(parents=True, exist_ok=True)
lb.write_csv("/app/output/lb.csv")
