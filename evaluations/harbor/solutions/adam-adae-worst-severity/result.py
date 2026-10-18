# Reference solution for the yamaa benchmark adam-adae-worst-severity (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import pandas as pd

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AEBODSYS",
    "AEDECOD",
    "ASTDT",
    "AESEV",
    "TRTEMFL",
    "ASEVN",
    "AOCCPIFL",
]

SEVERITY_RANK = {"MILD": 1, "MODERATE": 2, "SEVERE": 3}


def main() -> None:
    df = pd.read_csv("/app/input/adae_raw.csv", dtype=str, keep_default_na=False)

    df["ASEVN"] = df["AESEV"].map(SEVERITY_RANK)

    eligible = (df["TRTEMFL"] == "Y") & df["ASEVN"].notna()

    df["AOCCPIFL"] = ""
    if eligible.any():
        cand = df.loc[eligible].copy()
        cand["_astdt_key"] = cand["ASTDT"].replace("", "9999-12-31")
        cand["_aeseq_key"] = pd.to_numeric(cand["AESEQ"])
        # Worst severity first, then earliest start date, then lowest AESEQ.
        cand = cand.sort_values(
            ["USUBJID", "AEDECOD", "ASEVN", "_astdt_key", "_aeseq_key"],
            ascending=[True, True, False, True, True],
            kind="mergesort",
        )
        winners = cand.drop_duplicates(subset=["USUBJID", "AEDECOD"], keep="first")
        df.loc[winners.index, "AOCCPIFL"] = "Y"

    out = df[COLUMNS].copy()
    out["ASEVN"] = out["ASEVN"].apply(
        lambda v: "" if pd.isna(v) else str(int(v))
    )
    out.to_csv("/app/output/adae.csv", index=False)


if __name__ == "__main__":
    main()
