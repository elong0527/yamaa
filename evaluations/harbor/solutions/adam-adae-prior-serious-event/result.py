# Reference solution for the yamaa benchmark adam-adae-prior-serious-event (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

records = []
for subject, group in ae.sort(["USUBJID", "AESEQ"]).group_by("USUBJID", maintain_order=True):
    rows = group.sort("AESEQ").to_dicts()
    serious_seqs = [r["AESEQ"] for r in rows if r["AESER"] == "Y"]
    first_seq = min(serious_seqs) if serious_seqs else None
    first_term = next(
        (r["AEDECOD"] for r in rows if r["AESEQ"] == first_seq), None
    )
    by_seq = {r["AESEQ"]: r for r in rows}
    for row in rows:
        seq = row["AESEQ"]
        earlier_serious = [s for s in serious_seqs if s < seq]
        prior_seq = max(earlier_serious) if earlier_serious else None
        prior_term = by_seq.get(prior_seq, {}).get("AEDECOD") if prior_seq else None
        earlier_all = [r["AESEQ"] for r in rows if r["AESEQ"] < seq]
        prev_seq = max(earlier_all) if earlier_all else None
        prev_term = by_seq.get(prev_seq, {}).get("AEDECOD") if prev_seq else None
        is_serious = row["AESER"] == "Y"
        # A serious event leaves its own prior and first fields empty. A
        # serious event with no coded term still counts for the flag and
        # the sequence number, while the term fields stay empty.
        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AESEQ": seq,
                "AEDECOD": row["AEDECOD"],
                "AESER": row["AESER"],
                "PRIOR_SAEFL": None
                if is_serious or prior_seq is None
                else "Y",
                "PRIOR_SAEDECOD": None if is_serious else prior_term,
                "PRIOR_SAESEQ": None if is_serious else prior_seq,
                "FIRST_SAEDECOD": None if is_serious else first_term,
                "FIRST_SAESEQ": None if is_serious else first_seq,
                "PREV_AEDECOD": prev_term,
            }
        )

adae = pl.DataFrame(
    records,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "AESEQ": pl.Int64,
        "AEDECOD": pl.String,
        "AESER": pl.String,
        "PRIOR_SAEFL": pl.String,
        "PRIOR_SAEDECOD": pl.String,
        "PRIOR_SAESEQ": pl.Int64,
        "FIRST_SAEDECOD": pl.String,
        "FIRST_SAESEQ": pl.Int64,
        "PREV_AEDECOD": pl.String,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
