# Reference solution for the yamaa benchmark sdtm-cm-collected-medications (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import re
from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

wide = odm.filter(pl.col("ItemGroupOID") == "IG.CM").pivot(
    index=["StudyOID", "SubjectKey", "ItemGroupRepeatKey"],
    on="ItemOID",
    values="Value",
    aggregate_function="first",
)

# A plain number carries the dose; anything else (like a range 50-75) stays
# as text. Frequency and route labels map to controlled terminology.
def is_plain_number(value: str | None) -> bool:
    return value is not None and re.fullmatch(r"\d+(\.\d+)?", value.strip() or "") is not None


rows = []
for record in wide.to_dicts():
    dose_raw = (record.get("IT.CM.CMDSTXT") or "").strip() or None
    if dose_raw is not None and is_plain_number(dose_raw):
        dose, dosetxt = float(dose_raw), None
    else:
        dose, dosetxt = None, dose_raw
    freq_raw = record.get("IT.CM.CMDOSFRQ")
    freq = {"Once daily": "QD", "Twice daily": "BID"}.get(freq_raw, freq_raw) or None
    route_raw = record.get("IT.CM.CMROUTE")
    route = {"By mouth": "ORAL"}.get(route_raw, route_raw) or None
    prior = record.get("IT.CM.CMPRIOR")
    ongo = record.get("IT.CM.CMONGO")
    # BEFORE/SCREENING when the taken-before-study box is checked; ONGOING/
    # END OF STUDY when the ongoing box is checked. An ongoing medication
    # has no end date.
    endtc = record.get("IT.CM.CMENDTC") or None
    if ongo == "Y":
        endtc = None
    rows.append(
        {
            "DOMAIN": "CM",
            "STUDYID": record["StudyOID"],
            "USUBJID": record["SubjectKey"],
            "CMSEQ": int(record["ItemGroupRepeatKey"]),
            "CMTRT": record.get("IT.CM.CMTRT") or None,
            "CMINDC": record.get("IT.CM.CMINDC") or None,
            "CMDOSE": dose,
            "CMDOSTXT": dosetxt,
            "CMDOSU": record.get("IT.CM.CMDOSU") or None,
            "CMDOSFRQ": freq,
            "CMROUTE": route,
            "CMSTDTC": record.get("IT.CM.CMSTDTC") or None,
            "CMENDTC": endtc,
            "CMSTRTPT": "BEFORE" if prior == "Y" else None,
            "CMSTTPT": "SCREENING" if prior == "Y" else None,
            "CMENRTPT": "ONGOING" if ongo == "Y" else None,
            "CMENTPT": "END OF STUDY" if ongo == "Y" else None,
        }
    )

cm = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "CMSEQ": pl.Int64,
        "CMTRT": pl.String,
        "CMINDC": pl.String,
        "CMDOSE": pl.Float64,
        "CMDOSTXT": pl.String,
        "CMDOSU": pl.String,
        "CMDOSFRQ": pl.String,
        "CMROUTE": pl.String,
        "CMSTDTC": pl.String,
        "CMENDTC": pl.String,
        "CMSTRTPT": pl.String,
        "CMSTTPT": pl.String,
        "CMENRTPT": pl.String,
        "CMENTPT": pl.String,
    },
).sort(["USUBJID", "CMSEQ"])

Path("/app/output").mkdir(exist_ok=True)
cm.write_csv("/app/output/cm.csv")
