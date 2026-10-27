from pathlib import Path

import pandas as pd


INPUT_PATH = Path("/app/input/morphology.csv")
OUTPUT_PATH = Path("/app/output/mo.csv")
COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "MOSEQ",
    "MOTESTCD",
    "MOTEST",
    "MOORRES",
    "MOSTRESC",
    "MODTC",
]


def main():
    source = pd.read_csv(INPUT_PATH, dtype=str, keep_default_na=False)
    standard_results = {"normal": "NORMAL", "abnormal": "ABNORMAL"}
    mo = pd.DataFrame(
        {
            "DOMAIN": "MO",
            "STUDYID": source["STUDYID"],
            "USUBJID": source["USUBJID"],
            "MOSEQ": source["SEQ"],
            "MOTESTCD": "CELLTYPE",
            "MOTEST": "Cell Type",
            "MOORRES": source["RESULT"],
            "MOSTRESC": source["RESULT"].map(standard_results),
            "MODTC": source["DATE"],
        },
        columns=COLUMNS,
    )
    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    mo.to_csv(OUTPUT_PATH, index=False)


if __name__ == "__main__":
    main()
