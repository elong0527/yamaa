# Reference solution for the yamaa benchmark adam-adlb-limit-imputation (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv


def fmt(value):
    if value == int(value):
        return str(int(value))
    return repr(value)


def derive(text):
    """Return (AVAL text, DTYPE text) for a reported result string."""
    if text is None or text.strip() == "":
        return "", ""
    t = text.strip()
    for prefix, kind in (("<=", "le"), (">=", "ge"), ("<", "lt"), (">", "gt"), ("=", "eq")):
        if t.startswith(prefix):
            limit = float(t[len(prefix):])
            if kind == "lt":
                return fmt(limit - 1), "IMPUTATION"
            if kind == "gt":
                return fmt(limit + 1), "IMPUTATION"
            if kind in ("le", "ge"):
                return fmt(limit), "IMPUTATION"
            return fmt(limit), ""
    return fmt(float(t)), ""


def main():
    with open("/app/input/lb.csv", newline="") as f:
        rows = list(csv.DictReader(f))
    out = []
    for r in rows:
        aval, dtype = derive(r["LBSTRESC"])
        out.append({
            "STUDYID": r["STUDYID"],
            "USUBJID": r["USUBJID"],
            "LBSEQ": r["LBSEQ"],
            "PARAMCD": r["LBTESTCD"],
            "AVALC": r["LBSTRESC"],
            "AVAL": aval,
            "AVALU": r["LBSTRESU"],
            "DTYPE": dtype,
        })
    fields = ["STUDYID", "USUBJID", "LBSEQ", "PARAMCD", "AVALC", "AVAL", "AVALU", "DTYPE"]
    with open("/app/output/adlb.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        w.writerows(out)


if __name__ == "__main__":
    main()
