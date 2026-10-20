"""Benchmarks the 1.0 conversion gate excepts until their decided change lands.

The conversion gate of #3030 Phase 1 (P1-9, #3259, `make check-conversion`)
converts every benchmark to 1.0 and runs it: a positive must reproduce its
expected bytes, and a negative its condition, rule and context, with spec
paths changed only by the rename table's `paths` block. Decided items change
these cases, so they are excepted until P2-4, P2-7 and P1-11 land: D60
re-authors three negatives' expected errors for their 1.0 fault, D61 retires
two negatives with their conditions, and under D32 `sdtm-lb-metadata` binds
codelists without a project. Two more negatives change their first issue
under decided items that no re-authoring decision lists yet. The selftest case
list refuses every excepted benchmark too
(`tools/validation/check_selftest_cases.py`), since their expected files are
about to change.
"""

EXCEPTED = {
    "negative-date-diff-units": (
        "D58 (P-ORD-2): its context names diff.bounds values, which 1.0 renames and "
        "TIME-5 names as closed values (D71); P2-7 re-authors it"
    ),
    "negative-derivation-call-form": (
        "D18: its derivation is a call outside the closed vocabulary in 1.0 "
        "(prohibited_function); owner decision pending (#3283), P2-4"
    ),
    "negative-impute-missing-month": "D61: retires with month_required (P2-4)",
    "negative-impute-unreachable-month": "D61: retires with month_not_permitted (P2-4)",
    "negative-literal-structure": "D60: its expected error is re-authored (P2-4)",
    "negative-lookup-unknown-key": "D60: its expected error is re-authored (P2-4)",
    "negative-version-mismatch": "D60: its expected error is re-authored (P2-4)",
    "sdtm-lb-metadata": "D32: binds codelists without a project until P1-11",
}
