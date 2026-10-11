# Python reference for the yamaa benchmark adam-adae-worsening-emergence.
# Uses the collected event records and dated findings to construct periods.
from pathlib import Path

import polars as pl


def normalize(value):
    return value + ":00" if value and len(value) == 16 else value


ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).to_dicts()
fa = pl.read_csv("/app/input/fa.csv", infer_schema=False).to_dicts()
exposure = {
    (row["STUDYID"], row["USUBJID"]): normalize(row["TRTSDTM"])
    for row in pl.read_csv("/app/input/adsl.csv", infer_schema=False).to_dicts()
}
severity_rank = {"MILD": 1, "MODERATE": 2, "SEVERE": 3}
output = []
sequences = {}
for event in sorted(ae, key=lambda r: (r["STUDYID"], r["USUBJID"], int(r["AESEQ"]))):
    subject = event["STUDYID"], event["USUBJID"]
    trt = exposure.get(subject)
    onset, end = normalize(event["AESTDTC"]), normalize(event["AEENDTC"])
    changes = {}
    for finding in fa:
        if (finding["STUDYID"], finding["USUBJID"]) != subject:
            continue
        if finding["FALNKID"] != event["AELNKID"]:
            continue
        time = normalize(finding["FADTC"])
        values = changes.setdefault(time, {})
        test = finding["FATESTCD"]
        if test in values:
            raise ValueError("Duplicate finding at the same event and time")
        values[test] = finding["FASTRESC"]
    states = []
    sev, grade = None, None
    for time, values in sorted(changes.items()):
        if values.get("SEV") is not None:
            sev = values["SEV"]
        if values.get("TOXGR") is not None:
            grade = int(values["TOXGR"])
        states.append((time, sev, grade))
    before = [state for state in states if trt and state[0] < trt]
    baseline_sev = severity_rank.get(before[-1][1]) if before else None
    baseline_grade = before[-1][2] if before else None
    emergent = False
    for i, (time, sev, grade) in enumerate(states):
        if onset and trt:
            if onset >= trt:
                emergent = True
            elif time >= trt:
                rank = severity_rank.get(sev)
                sev_worse = (
                    rank is not None
                    and baseline_sev is not None
                    and rank > baseline_sev
                )
                grade_worse = (
                    grade is not None
                    and baseline_grade is not None
                    and grade > baseline_grade
                )
                emergent = emergent or sev_worse or grade_worse
        sequences[subject] = sequences.get(subject, 0) + 1
        output.append(
            {
                "STUDYID": event["STUDYID"],
                "USUBJID": event["USUBJID"],
                "ASEQ": sequences[subject],
                "AESEQ": int(event["AESEQ"]),
                "AEDECOD": event["AEDECOD"],
                "AESTDTC": onset,
                "AEENDTC": end,
                "ASTDTM": time,
                "AENDTM": states[i + 1][0] if i + 1 < len(states) else end,
                "TRTSDTM": trt,
                "AESEV": event["AESEV"],
                "AETOXGR": event["AETOXGR"],
                "ASEV": sev,
                "ATOXGRN": grade,
                "TRTEMFL": "Y" if emergent else None,
            }
        )

Path("/app/output").mkdir(exist_ok=True)
pl.DataFrame(output).write_csv("/app/output/adae.csv")
