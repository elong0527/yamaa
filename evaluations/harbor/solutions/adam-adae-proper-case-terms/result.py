# Reference solution for the yamaa benchmark adam-adae-proper-case-terms (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT = "/app/input/ae.csv"
OUTPUT = "/app/output/adae.csv"

COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "AETERM", "AETERMPC", "AETERMPB"]

# SAS PROPCASE's default delimiters, and the ones that replace them for
# AETERMPB.
DEFAULT = " /-(.\t"
COMMA_AND_BRACKET = " /-(.,["


def propcase(text, delimiters):
    """The first character, and each character that directly follows one of
    the delimiters, in upper case; every other letter in lower case."""
    return "".join(
        character.upper()
        if index == 0 or text[index - 1] in delimiters
        else character.lower()
        for index, character in enumerate(text)
    )


def main():
    with open(INPUT, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    out_rows = []
    for row in rows:
        term = row.get("AETERM") or ""
        out_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AESEQ": row["AESEQ"],
                "AETERM": term,
                "AETERMPC": propcase(term, DEFAULT) if term else "Not Reported",
                "AETERMPB": (
                    propcase(term, COMMA_AND_BRACKET) if term else "Not Reported"
                ),
            }
        )

    with open(OUTPUT, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
