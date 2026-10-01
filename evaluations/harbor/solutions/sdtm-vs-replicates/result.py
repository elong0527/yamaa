# Reference solution for the yamaa benchmark sdtm-vs-replicates (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def fmt_text(x: float | None) -> str | None:
    if x is None:
        return None
    import math

    if isinstance(x, float) and (math.isnan(x) or math.isinf(x)):
        return None
    if float(x).is_integer():
        return str(int(float(x)))
    return f"{float(x):.15g}"


odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

test_name = {
    "IT.VS.SYSBP": ("SYSBP", "Systolic Blood Pressure"),
    "IT.VS.DIABP": ("DIABP", "Diastolic Blood Pressure"),
}
visit_name = {"SE.VISIT1": "VISIT 1", "SE.VISIT2": "VISIT 2"}

# Collected readings: one per subject, visit, test, and replicate. A blank
# value or an absent record gives no reading record.
readings: dict[tuple[str, str, str, str, str], float] = {}
studies: dict[str, str] = {}
for row in odm.to_dicts():
    if row["ItemOID"] not in test_name or row["StudyEventOID"] not in visit_name:
        continue
    value = row["Value"]
    if value is None or value == "":
        continue
    studies[row["SubjectKey"]] = row["StudyOID"]
    key = (
        row["SubjectKey"],
        row["StudyEventOID"],
        row["ItemOID"],
        row["ItemGroupRepeatKey"],
    )
    readings[key] = float(value)
    testcd, _ = test_name[row["ItemOID"]]
    _ = testcd

records = []
# By visit name, then DIABP before SYSBP; each test's readings in replicate
# order, then its mean record.
for subject in sorted(studies):
    for event in sorted(visit_name, key=lambda e: visit_name[e]):
        visit = visit_name[event]
        for item in ("IT.VS.DIABP", "IT.VS.SYSBP"):
            testcd, test = test_name[item]
            reps = sorted(
                (rep, val)
                for (s, e, i, rep), val in readings.items()
                if s == subject and e == event and i == item
            )
            if not reps:
                continue
            for rep, val in reps:
                records.append(
                    {
                        "DOMAIN": "VS",
                        "STUDYID": studies[subject],
                        "USUBJID": subject,
                        "VISIT": visit,
                        "VSTESTCD": testcd,
                        "VSTEST": test,
                        "VSREPNUM": int(rep),
                        "VSORRES": fmt_text(val),
                        "VSORRESU": "mmHg",
                        "VSSTRESN": val,
                        "VSSTRESC": fmt_text(val),
                        "VSDRVFL": None,
                        "_visit_ord": visit,
                        "_test_ord": testcd,
                        "_rep_ord": int(rep),
                        "_mean_last": 0,
                    }
                )
            mean = sum(val for _, val in reps) / len(reps)
            records.append(
                {
                    "DOMAIN": "VS",
                    "STUDYID": studies[subject],
                    "USUBJID": subject,
                    "VISIT": visit,
                    "VSTESTCD": testcd,
                    "VSTEST": test,
                    "VSREPNUM": None,
                    "VSORRES": fmt_text(mean),
                    "VSORRESU": "mmHg",
                    "VSSTRESN": mean,
                    "VSSTRESC": fmt_text(mean),
                    "VSDRVFL": "Y",
                    "_visit_ord": visit,
                    "_test_ord": testcd,
                    "_rep_ord": 999,
                    "_mean_last": 1,
                }
            )

vs = (
    pl.DataFrame(records)
    .sort(["USUBJID", "_visit_ord", "_test_ord", "_mean_last", "_rep_ord"])
    .with_columns(VSSEQ=pl.int_range(1, pl.len() + 1).over("USUBJID"))
    .sort(["USUBJID", "VSSEQ"])
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "VSSEQ",
        "VISIT",
        "VSTESTCD",
        "VSTEST",
        "VSREPNUM",
        "VSORRES",
        "VSORRESU",
        "VSSTRESN",
        "VSSTRESC",
        "VSDRVFL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
vs.write_csv("/app/output/vs.csv")
