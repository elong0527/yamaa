# Reference solution for the yamaa benchmark sdtm-suppmh-long-terms (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build SUPPMH from MH_RAW: continuation pieces of MHTERM beyond 200 characters."""
import os

import pandas as pd

INPUT = "/app/input"
OUTPUT = "/app/output"
LIMIT = 200
COLUMNS = ["STUDYID", "RDOMAIN", "USUBJID", "IDVAR", "IDVARVAL",
           "QNAM", "QLABEL", "QVAL", "QORIG", "QEVAL"]


def split_term(text, limit=LIMIT):
    """Split text into pieces of at most `limit` characters at word boundaries.

    The space at each break belongs to neither piece; a word ending exactly
    at the limit fits.
    """
    pieces = []
    rest = text
    while len(rest) > limit:
        idx = rest.rfind(" ", 0, limit + 1)
        if idx <= 0:
            # No whole word fits: fall back to a hard break.
            pieces.append(rest[:limit])
            rest = rest[limit:]
            continue
        pieces.append(rest[:idx])
        rest = rest[idx + 1:]
    pieces.append(rest)
    return pieces


def main():
    mh = pd.read_csv(os.path.join(INPUT, "mh_raw.csv"), dtype=str,
                     keep_default_na=False)
    rows = []
    for rec in mh.itertuples(index=False):
        pieces = split_term(rec.MHTERM)
        seq = str(int(float(rec.MHSEQ))) if rec.MHSEQ.strip() else ""
        for n, piece in enumerate(pieces[1:], start=1):
            rows.append({
                "STUDYID": rec.STUDYID,
                "RDOMAIN": "MH",
                "USUBJID": rec.USUBJID,
                "IDVAR": "MHSEQ",
                "IDVARVAL": seq,
                "QNAM": f"MHTERM{n}",
                "QLABEL": f"Reported Term for the Medical History {n}",
                "QVAL": piece,
                "QORIG": "Collected",
                "QEVAL": "",
            })
    out = pd.DataFrame(rows, columns=COLUMNS)
    os.makedirs(OUTPUT, exist_ok=True)
    out.to_csv(os.path.join(OUTPUT, "suppmh.csv"), index=False)


if __name__ == "__main__":
    main()
