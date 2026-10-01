# Reference solution for the yamaa benchmark sdtm-vs-units (Python track).
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
    fx = float(x)
    if fx.is_integer():
        return str(int(fx))
    text = f"{fx:.15g}"
    return text


def fmt_orres(x: str | None) -> str | None:
    if x is None or x == "":
        return None
    try:
        return fmt_text(float(x))
    except ValueError:
        return x


raw = pl.read_csv("/app/input/vs_raw.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64),
)

rows = raw.to_dicts()
for row in rows:
    orres = row["VSORRES"]
    has_result = orres is not None and orres != ""
    test = row["VSTESTCD"]
    unit = row["VSORRESU"]
    if not has_result:
        row["VSORRES"] = None
        row["VSORRESU"] = None
        row["VSSTRESN"] = None
        row["VSSTRESC"] = None
        row["VSSTRESU"] = None
        row["VSSTAT"] = "NOT DONE"
        continue
    row["VSORRES"] = fmt_orres(orres)
    value = float(orres)
    if test == "HEIGHT":
        std = value
        std_unit = "cm"
    elif test == "WEIGHT":
        std = value * 0.45359237 if unit == "LB" else value
        std_unit = "kg"
    else:
        std = (value - 32) * 5 / 9 if unit == "F" else value
        std_unit = "C"
    row["VSSTRESN"] = std
    row["VSSTRESC"] = fmt_text(std)
    row["VSSTRESU"] = std_unit
    # Original unit is empty when no result was collected; otherwise as
    # collected.
    row["VSORRESU"] = unit
    row["VSSTAT"] = None

vs = (
    pl.DataFrame(rows)
    .with_columns(
        DOMAIN=pl.lit("VS"),
        _ord=pl.col("VSTESTCD").replace_strict(
            {"HEIGHT": 1, "WEIGHT": 2, "TEMP": 3},
            default=9,
            return_dtype=pl.Int64,
        ),
    )
    .sort(["_ord", "USUBJID", "VSSEQ"])
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
        "VSSTAT",
    )
)

Path("/app/output").mkdir(exist_ok=True)
vs.write_csv("/app/output/vs.csv")
