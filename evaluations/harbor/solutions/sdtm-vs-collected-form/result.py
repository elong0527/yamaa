# Reference solution for the yamaa benchmark sdtm-vs-collected-form (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
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
tests = pl.read_csv("/app/input/vs_tests.csv", infer_schema=False).with_columns(
    pl.col("TESTORD").cast(pl.Int64),
)

test_list = sorted(tests.to_dicts(), key=lambda r: r["TESTORD"])

# One wide form per subject per visit: its date, position, method, not-done
# flag, reason, and each test's result and unit.
visits: dict[tuple[str, str], dict] = {}
for row in odm.to_dicts():
    key = (row["SubjectKey"], row["StudyEventOID"])
    entry = visits.setdefault(
        key,
        {
            "study": row["StudyOID"],
            "subject": row["SubjectKey"],
            "visit": row["StudyEventOID"],
            "items": {},
        },
    )
    entry["items"][row["ItemOID"]] = row["Value"]

records = []
for entry in visits.values():
    items = entry["items"]
    vsdtc = items.get("IT.VS.VSDTC")
    position = items.get("IT.VS.POSITION")
    method = items.get("IT.VS.TEMPMETHOD")
    not_done = items.get("IT.VS.BPNOTDONE") == "Y"
    reason = items.get("IT.VS.BPREASND")
    for test in test_list:
        testcd = test["VSTESTCD"]
        result_item = f"IT.VS.{testcd}"
        unit_item = f"IT.VS.{testcd}U"
        # WEIGHT unit in the form is IT.VS.WEIGHTU, which fits the pattern;
        # the same holds for every test.
        result = items.get(result_item)
        unit = items.get(unit_item)
        has_result = result is not None and result != ""
        if testcd in ("SYSBP", "DIABP") and not_done:
            has_result = False
            result = None
            unit = None
        if not has_result:
            orres = None
            orresu = None
            stresn = None
            stresc = None
            stresu = None
            pos = None
            meth = None
        else:
            orres = result
            orresu = unit
            stresn = float(result)
            stresc = fmt_text(stresn)
            stresu = unit
            pos = position if test["HAS_POSITION"] == "1" else None
            meth = method if test["HAS_METHOD"] == "1" else None
        if test["HAS_NOTDONE"] == "1" and not_done:
            stat = "NOT DONE"
            reasnd = reason
        else:
            stat = None
            reasnd = None
        records.append(
            {
                "DOMAIN": "VS",
                "STUDYID": entry["study"],
                "USUBJID": entry["subject"],
                "VISIT": entry["visit"],
                "VSTESTCD": testcd,
                "VSTEST": test["VSTEST"],
                "_testord": test["TESTORD"],
                "VSORRES": orres,
                "VSORRESU": orresu,
                "VSSTRESN": stresn,
                "VSSTRESC": stresc,
                "VSSTRESU": stresu,
                "VSPOS": pos,
                "VSMETHOD": meth,
                "VSSTAT": stat,
                "VSREASND": reasnd,
                "VSDTC": vsdtc,
                "_vsdtc": vsdtc or "",
            }
        )

vs = (
    pl.DataFrame(records)
    .sort(["USUBJID", "_vsdtc", "_testord"])
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
        "VSORRES",
        "VSORRESU",
        "VSSTRESN",
        "VSSTRESC",
        "VSSTRESU",
        "VSPOS",
        "VSMETHOD",
        "VSSTAT",
        "VSREASND",
        "VSDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
vs.write_csv("/app/output/vs.csv")
