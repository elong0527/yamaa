# Reference solution for the yamaa benchmark sdtm-fa-reactogenicity-diary (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the SDTM FA (reactogenicity) dataset from the DIARY dataset.

One record per test for each solicited reaction on each diary day:
- an OCCUR record for every reaction on every diary day,
- a SEV record on completed diary days,
- an LDIAM record when redness or swelling occurred on a completed day.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

TEST_LABELS = {
    "OCCUR": "Occurrence Indicator",
    "SEV": "Severity/Intensity",
    "LDIAM": "Longest Diameter",
}
TEST_ORDER = {"OCCUR": 0, "SEV": 1, "LDIAM": 2}
DIAMETER_REACTIONS = {"REDNESS", "SWELLING"}

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "FASEQ", "FATESTCD", "FATEST",
    "FAOBJ", "FACAT", "FASCAT", "FAORRES", "FAORRESU", "FASTRESC",
    "FASTRESN", "FASTRESU", "FASTAT", "FATPT", "FADTC",
]


def clean(value):
    """Return the value as text, with missing values as empty strings."""
    if value is None or (isinstance(value, float) and pd.isna(value)):
        return ""
    text = str(value).strip()
    return "" if text.lower() == "nan" else text


def format_number(value):
    """Render a measured number without a trailing .0 when integral."""
    number = float(value)
    if number == int(number):
        return str(int(number))
    return str(number)


def main():
    diary = pd.read_csv(f"{INPUT_DIR}/diary.csv", dtype=str, keep_default_na=False)

    records = []
    for _, row in diary.iterrows():
        completed = clean(row["COMPLETED"]).upper() == "Y"
        missed = not completed
        reaction = clean(row["REACTION"])
        catsrc = clean(row["CATSRC"]).upper()
        fasCAT = "ADMINISTRATION SITE" if catsrc == "LOCAL" else "SYSTEMIC"
        occur = clean(row["OCCUR"])
        sev = clean(row["SEV"])

        base = {
            "DOMAIN": "FA",
            "STUDYID": clean(row["STUDYID"]),
            "USUBJID": clean(row["USUBJID"]),
            "FAOBJ": reaction,
            "FACAT": "REACTOGENICITY",
            "FASCAT": fasCAT,
            "FASTAT": "NOT DONE" if missed else "",
            "FATPT": clean(row["TPT"]),
            "FADTC": clean(row["DTC"]),
            "_day": int(float(row["DIARYDAY"])),
            "_reaction": reaction,
        }

        # OCCUR record: every reaction on every diary day.
        occur_res = "" if missed else occur
        records.append({
            **base,
            "FATESTCD": "OCCUR",
            "FATEST": TEST_LABELS["OCCUR"],
            "FAORRES": occur_res,
            "FAORRESU": "",
            "FASTRESC": occur_res,
            "FASTRESN": "",
            "FASTRESU": "",
        })

        if completed:
            # SEV record: recorded severity, or NONE when it did not occur.
            sev_res = sev if sev else ("NONE" if occur.upper() == "N" else "")
            records.append({
                **base,
                "FATESTCD": "SEV",
                "FATEST": TEST_LABELS["SEV"],
                "FAORRES": sev_res,
                "FAORRESU": "",
                "FASTRESC": sev_res,
                "FASTRESN": "",
                "FASTRESU": "",
            })

            # LDIAM record: redness or swelling that occurred.
            if reaction in DIAMETER_REACTIONS and occur.upper() == "Y":
                diameter = clean(row["DIAMETER"])
                unit = clean(row["DIAMUNIT"])
                records.append({
                    **base,
                    "FATESTCD": "LDIAM",
                    "FATEST": TEST_LABELS["LDIAM"],
                    "FAORRES": diameter,
                    "FAORRESU": unit,
                    "FASTRESC": diameter,
                    "FASTRESN": format_number(diameter) if diameter else "",
                    "FASTRESU": unit,
                })

    fa = pd.DataFrame(records)
    fa["_test_order"] = fa["FATESTCD"].map(TEST_ORDER)
    fa = fa.sort_values(
        ["USUBJID", "_day", "_reaction", "_test_order"],
        kind="stable",
    ).reset_index(drop=True)
    fa["FASEQ"] = fa.groupby("USUBJID").cumcount() + 1
    fa = fa[COLUMNS]
    fa.to_csv(f"{OUTPUT_DIR}/fa.csv", index=False)


if __name__ == "__main__":
    main()
