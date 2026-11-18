# Reference solution for the yamaa benchmark adam-adppk-dose-intervals (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import datetime, timedelta

import pandas as pd

INTERVAL_HOURS = {"QD": 24, "BID": 12}


def parse(dtc):
    """Return (datetime, has_time) for an ISO 8601 date or date/time, or (None, False)."""
    dtc = (dtc or "").strip()
    if not dtc:
        return None, False
    if "T" in dtc:
        date, time = dtc.split("T", 1)
        parts = [int(p) for p in time.split(":") if p != ""]
        if parts:
            parts += [0] * (3 - len(parts))
            return datetime.fromisoformat(date) + timedelta(
                hours=parts[0], minutes=parts[1], seconds=parts[2]
            ), True
        dtc = date
    return datetime.fromisoformat(dtc), False


def derive(row):
    start, start_has_time = parse(row["EXSTDTC"])
    end, end_has_time = parse(row["EXENDTC"])
    if end is not None and not end_has_time:
        end += timedelta(days=1) - timedelta(seconds=1)

    interval = INTERVAL_HOURS.get(row["EXDOSFRQ"].strip().upper())
    addl = 0
    if interval is not None and start is not None and end is not None and end >= start:
        addl = int((end - start) // timedelta(hours=interval))
    ii = interval if addl > 0 else 0

    return pd.Series(
        {
            "ADTM": start.strftime("%Y-%m-%dT%H:%M:%S") if start is not None else "",
            "ATMF": "" if start_has_time or start is None else "H",
            "EVID": 1,
            "AMT": row["EXDOSE"],
            "II": ii,
            "ADDL": addl,
        }
    )


def main():
    ex = pd.read_csv(
        "/app/input/ex.csv",
        dtype={"STUDYID": str, "USUBJID": str, "EXDOSFRQ": str, "EXSTDTC": str, "EXENDTC": str},
        keep_default_na=False,
        na_values={"EXDOSE": [""]},
    )
    ex = ex[ex["EXDOSE"] > 0].reset_index(drop=True)
    derived = ex.apply(derive, axis=1)
    out = pd.concat([ex[["STUDYID", "USUBJID", "EXSEQ"]], derived], axis=1)
    out["AMT"] = ex["EXDOSE"]
    out = out.astype({"EVID": int, "II": int, "ADDL": int})
    out = out[["STUDYID", "USUBJID", "EXSEQ", "ADTM", "ATMF", "EVID", "AMT", "II", "ADDL"]]
    out.to_csv("/app/output/adppk.csv", index=False)


if __name__ == "__main__":
    main()
