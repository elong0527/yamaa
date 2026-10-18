# Reference solution for the yamaa benchmark sdtm-ae-meddra-hierarchy (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM AE dataset from AE_RAW and MEDDRA_SYNTHETIC.

One record per adverse event. The coded hierarchy for each record comes
from the MEDDRA_SYNTHETIC row that holds the record's AELLTCD on the
primary path (PRIMARY_SOC == "Y"); secondary paths are never used.
"""

import polars as pl

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AETERM",
    "AELLT",
    "AELLTCD",
    "AEDECOD",
    "AEPTCD",
    "AEHLT",
    "AEHLTCD",
    "AEHLGT",
    "AEHLGTCD",
    "AEBODSYS",
    "AEBODSCD",
    "AESOC",
    "AESOCCD",
]


def main() -> None:
    ae_raw = pl.read_csv(f"{INPUT_DIR}/ae_raw.csv", infer_schema=False)
    meddra = pl.read_csv(
        f"{INPUT_DIR}/meddra_synthetic.csv", infer_schema=False
    )

    # Primary path only: one row per LLT code.
    hierarchy = {}
    for row in meddra.filter(pl.col("PRIMARY_SOC") == "Y").iter_rows(named=True):
        hierarchy[row["LLTCD"]] = row

    records = []
    for ae in ae_raw.iter_rows(named=True):
        lltcd = ae["AELLTCD"] or None
        med = hierarchy.get(lltcd) if lltcd else None
        if med is None:
            # No assigned code, or no dictionary row: AELLT and every
            # hierarchy column are blank; AELLTCD keeps the code as
            # collected.
            llt = pt = ptcd = hlt = hltcd = None
            hlgt = hlgtcd = soc = soccd = None
        else:
            llt = med["LLTNAME"]
            pt, ptcd = med["PTNAME"], med["PTCD"]
            hlt, hltcd = med["HLTNAME"], med["HLTCD"]
            hlgt, hlgtcd = med["HLGTNAME"], med["HLGTCD"]
            soc, soccd = med["SOCNAME"], med["SOCCD"]
        records.append(
            {
                "DOMAIN": "AE",
                "STUDYID": ae["STUDYID"],
                "USUBJID": ae["USUBJID"],
                "AESEQ": ae["AESEQ"],
                "AETERM": ae["AETERM"],
                "AELLT": llt,
                "AELLTCD": lltcd,
                "AEDECOD": pt,
                "AEPTCD": ptcd,
                "AEHLT": hlt,
                "AEHLTCD": hltcd,
                "AEHLGT": hlgt,
                "AEHLGTCD": hlgtcd,
                "AEBODSYS": soc,
                "AEBODSCD": soccd,
                "AESOC": soc,
                "AESOCCD": soccd,
            }
        )

    pl.DataFrame(records, schema={c: pl.Utf8 for c in COLUMNS}).write_csv(
        f"{OUTPUT_DIR}/ae.csv"
    )


if __name__ == "__main__":
    main()
