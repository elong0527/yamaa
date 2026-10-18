# Reference solution for the yamaa benchmark adam-adae-severity-rank (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADAE from AE with severity numeric code and per-subject ranks.

ASEVN maps ASEV (MILD/MODERATE/SEVERE) to 1/2/3. SEVRANK is the
competition rank of the event within the subject, worst severity first;
events without a reported severity rank after all reported ones and
share a single rank. SEVLVL is the dense rank, with an unreported
severity counting as one further distinct level.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adae.csv"

SEV_NUM = {"MILD": 1, "MODERATE": 2, "SEVERE": 3}


def main() -> None:
    ae = pd.read_csv(f"{INPUT_DIR}/ae.csv", dtype=str, keep_default_na=False)

    ae["ASEV"] = ae["AESEV"]
    ae["ASEVN"] = ae["ASEV"].map(SEV_NUM)

    sevrank = pd.Series(index=ae.index, dtype=int)
    sevlvl = pd.Series(index=ae.index, dtype=int)
    for _, group in ae.groupby("USUBJID", sort=False):
        values = group["ASEVN"]
        reported = sorted({v for v in values if pd.notna(v)}, reverse=True)
        n_reported_events = int(values.notna().sum())
        # Dense level per reported severity; unreported is one past the last.
        dense = {v: i + 1 for i, v in enumerate(reported)}
        blank_dense = len(reported) + 1
        blank_rank = n_reported_events + 1
        for idx, v in values.items():
            if pd.notna(v):
                sevrank[idx] = 1 + int((values > v).sum())
                sevlvl[idx] = dense[v]
            else:
                sevrank[idx] = blank_rank
                sevlvl[idx] = blank_dense
    ae["SEVRANK"] = sevrank
    ae["SEVLVL"] = sevlvl

    out = ae[["STUDYID", "USUBJID", "AESEQ", "ASEV", "ASEVN", "SEVRANK", "SEVLVL"]]
    out = out.assign(
        ASEVN=out["ASEVN"].map(lambda v: "" if pd.isna(v) else int(v)),
        SEVRANK=out["SEVRANK"].astype(int),
        SEVLVL=out["SEVLVL"].astype(int),
    )
    out.to_csv(OUTPUT_PATH, index=False)


if __name__ == "__main__":
    main()
