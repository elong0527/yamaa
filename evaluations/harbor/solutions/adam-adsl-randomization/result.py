# Reference solution for the yamaa benchmark adam-adsl-randomization (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the ADSL randomization dataset from the DM (and ODM) inputs.

One record per subject with columns:
STUDYID, USUBJID, RANDDT, RANDDY, RANDDTC

- RANDDT: the collected randomization date.
- RANDDY: study day of RANDDT measured from RFSTDTC (the reference start
  date): on or after RFSTDTC the day is the difference in days plus 1,
  before RFSTDTC it is the (negative) difference in days. Blank when
  RANDDT or RFSTDTC has no value.
- RANDDTC: the collected randomization moment RANDDTTM as text at
  whole-second precision, date and time joined by T with the seconds
  always shown. Written from the collected moment independently of the
  collected date; blank when no moment was collected.
"""

import pandas as pd

INPUT_DIR = "/app/input"
OUTPUT_FILE = "/app/output/adsl.csv"


def main() -> None:
    dm = pd.read_parquet(f"{INPUT_DIR}/dm.parquet")

    rows = []
    for rec in dm.to_dict("records"):
        randdt = rec["RANDDT"]
        rfstdtc = rec["RFSTDTC"]
        randdttm = rec["RANDDTTM"]

        randdt_text = "" if pd.isna(randdt) else randdt.isoformat()

        if pd.isna(randdt) or pd.isna(rfstdtc):
            randdy = ""
        else:
            diff = (randdt - rfstdtc).days
            randdy = diff + 1 if diff >= 0 else diff

        randdtc = "" if pd.isna(randdttm) else randdttm.strftime("%Y-%m-%dT%H:%M:%S")

        rows.append(
            {
                "STUDYID": rec["STUDYID"],
                "USUBJID": rec["USUBJID"],
                "RANDDT": randdt_text,
                "RANDDY": randdy,
                "RANDDTC": randdtc,
            }
        )

    adsl = pd.DataFrame(rows, columns=["STUDYID", "USUBJID", "RANDDT", "RANDDY", "RANDDTC"])
    adsl.to_csv(OUTPUT_FILE, index=False)


if __name__ == "__main__":
    main()
