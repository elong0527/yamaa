# Reference solution for the yamaa benchmark sdtm-co-long-comments (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM CO (Comments) domain from CO_RAW.

One record per subject per collected comment number. The comment text is
split into 200-character pieces at fixed character positions:
COVAL = characters 1-200, COVAL1 = 201-400, COVAL2 = 401-600.
A blank comment gives no record. COSEQ keeps the collected comment number.
"""

from pathlib import Path

import pandas as pd

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")

PIECE = 200
COLUMNS = ["DOMAIN", "STUDYID", "USUBJID", "COSEQ", "COVAL", "COVAL1", "COVAL2"]


def main() -> None:
    raw = pd.read_csv(
        INPUT_DIR / "co_raw.csv",
        dtype=str,
        keep_default_na=False,
        na_values=[],
        encoding="utf-8",
    )

    records = []
    for row in raw.itertuples(index=False):
        comment = row.COMMENT if isinstance(row.COMMENT, str) else ""
        # A comment left blank (empty or only whitespace) gives no CO record.
        if comment.strip() == "":
            continue

        pieces = [comment[i * PIECE:(i + 1) * PIECE] for i in range(3)]
        if len(comment) > 3 * PIECE:
            raise ValueError(
                f"Comment for {row.USUBJID} COSEQ {row.COSEQ} has "
                f"{len(comment)} characters, more than COVAL-COVAL2 can hold"
            )

        records.append(
            {
                "DOMAIN": "CO",
                "STUDYID": row.STUDYID,
                "USUBJID": row.USUBJID,
                "COSEQ": int(str(row.COSEQ).strip()),
                "COVAL": pieces[0],
                "COVAL1": pieces[1] if pieces[1] != "" else None,
                "COVAL2": pieces[2] if pieces[2] != "" else None,
            }
        )

    co = pd.DataFrame.from_records(records, columns=COLUMNS)
    co["COSEQ"] = co["COSEQ"].astype("Int64")
    co = co.sort_values(["STUDYID", "USUBJID", "COSEQ"], kind="stable")
    co = co.reset_index(drop=True)

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    co.to_csv(OUTPUT_DIR / "co.csv", index=False, encoding="utf-8")


if __name__ == "__main__":
    main()
