# Reference solution for the yamaa benchmark sdtm-rs-timepoint-response (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value).strip() != ""


odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

visits: dict[tuple, dict] = {}
tr_target: dict[tuple, list] = {}
tr_any: dict[tuple, int] = {}
tu_target_ids: dict[str, set] = {}
baseline_target_diams: dict[str, list] = {}

for row in odm.to_dicts():
    subj, event = str(row["SubjectKey"]), str(row["StudyEventOID"])
    group, oid = str(row["ItemGroupOID"]), str(row["ItemOID"])
    value = row["Value"]
    key = (subj, event)
    if group == "IG.VISIT":
        visits.setdefault(
            key, {"studyid": str(row["StudyOID"]), "usubjid": subj}
        )[oid] = str(value) if _present(value) else None
    elif "TARGET" in group and group.startswith("IG.TR"):
        # Target tumor measurements at this visit.
        rep = str(row["ItemGroupRepeatKey"])
        tr_target.setdefault((subj, event, rep), {})[oid] = (
            str(value) if _present(value) else None
        )
        tr_any[key] = tr_any.get(key, 0) + 0
    elif group.startswith("IG.TR"):
        # Non-target measurements still count as done.
        tr_any[key] = tr_any.get(key, 0) + 1
    elif (
        "TARGET" in group
        and group.startswith("IG.TU")
        and event == "BASELINE"
        and oid == "IT.TU.TULNKID"
        and _present(value)
    ):
        tu_target_ids.setdefault(subj, set()).add(str(value).strip())

# Any TR row (target or not) means the assessment has measurement records.
for (subj, event, _rep) in list(tr_target.keys()):
    tr_any[(subj, event)] = tr_any.get((subj, event), 0) + 1

# Chosen target lesions from the baseline tumor identification, else the
# baseline target measurements; none means non-target disease only.
chosen: dict[str, int] = {}
for subj in {s for s, _e in visits}:
    if subj in tu_target_ids:
        chosen[subj] = len(tu_target_ids[subj])
    else:
        baseline_reps = {rep for (s, e, rep) in tr_target if s == subj and e == "BASELINE"}
        chosen[subj] = len(baseline_reps)

baseline_sum: dict[str, float | None] = {}
for subj in {s for s, _e in visits}:
    diams = []
    missing = False
    for (s, e, _rep), vals in tr_target.items():
        if s != subj or e != "BASELINE":
            continue
        diam = vals.get("IT.TR.LDIAM")
        if not _present(diam):
            missing = True
        else:
            try:
                diams.append(float(str(diam)))
            except ValueError:
                missing = True
    if missing or not diams:
        # No percent change can be computed from a missing baseline sum.
        # An empty baseline (non-target only) is handled as NE elsewhere.
        baseline_sum[subj] = None if (missing or not diams) and chosen.get(subj, 0) else 0.0
        if chosen.get(subj, 0) == 0:
            baseline_sum[subj] = None
        elif not missing and diams:
            baseline_sum[subj] = sum(diams)
    else:
        baseline_sum[subj] = sum(diams)

rows = []
by_subject: dict[str, list] = {}
for (subj, event), visit in visits.items():
    by_subject.setdefault(subj, []).append((event, visit))
for subj, items in by_subject.items():
    items.sort(key=lambda kv: int(kv[1].get("IT.VISIT.AVISITN") or 0))
    for seq, (event, visit) in enumerate(items, start=1):
        avisit = visit.get("IT.VISIT.AVISIT")
        avisitn = visit.get("IT.VISIT.AVISITN")
        adt = visit.get("IT.VISIT.ADT")
        has_records = tr_any.get((subj, event), 0) > 0
        if not has_records:
            # A scheduled assessment with no tumor measurement records.
            rsstresc, rsstat = None, "NOT DONE"
        elif event == "BASELINE":
            # A baseline assessment is never compared against itself.
            rsstresc, rsstat = "NE", None
        elif chosen.get(subj, 0) == 0:
            rsstresc, rsstat = "NE", None
        else:
            post_vals = [
                vals.get("IT.TR.LDIAM")
                for (s, e, _r), vals in tr_target.items()
                if s == subj and e == event
            ]
            measured = [v for v in post_vals if _present(v)]
            if len(measured) < chosen[subj]:
                rsstresc, rsstat = "NE", None
            else:
                base = baseline_sum.get(subj)
                if base is None or base == 0:
                    rsstresc, rsstat = "NE", None
                else:
                    try:
                        post = sum(float(str(v)) for v in measured)
                    except ValueError:
                        rsstresc, rsstat = "NE", None
                    else:
                        if all(float(str(v)) == 0 for v in measured):
                            rsstresc = "CR"
                        elif (base - post) / base >= 0.30:
                            rsstresc = "PR"
                        elif (post - base) / base >= 0.20:
                            rsstresc = "PD"
                        else:
                            rsstresc = "SD"
                        rsstat = None
        rows.append(
            {
                "STUDYID": visit["studyid"],
                "USUBJID": visit["usubjid"],
                "AVISIT": avisit,
                "AVISITN": avisitn,
                "ADT": adt,
                "RSSEQ": str(seq),
                "RSTESTCD": "TRGRESP",
                "RSTEST": "Timepoint Response",
                "RSSTRESC": rsstresc,
                "RSSTAT": rsstat,
            }
        )

rs = (
    pl.DataFrame(
        rows,
        schema={
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "AVISIT": pl.String,
            "AVISITN": pl.String,
            "ADT": pl.String,
            "RSSEQ": pl.String,
            "RSTESTCD": pl.String,
            "RSTEST": pl.String,
            "RSSTRESC": pl.String,
            "RSSTAT": pl.String,
        },
    )
    .with_columns(
        pl.col("RSSEQ").cast(pl.Int64).alias("_seq"),
        pl.col("AVISITN").cast(pl.Int64).alias("_visit"),
    )
    .sort(["USUBJID", "_visit"])
    .select(
        "STUDYID", "USUBJID", "AVISIT", "AVISITN", "ADT", "RSSEQ",
        "RSTESTCD", "RSTEST", "RSSTRESC", "RSSTAT",
    )
)

Path("/app/output").mkdir(exist_ok=True)
rs.write_csv("/app/output/rs.csv")
