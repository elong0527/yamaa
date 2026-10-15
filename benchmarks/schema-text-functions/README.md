# Text Operations

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-text-functions.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `UPPERTXT`, `LOWERTXT`, `EQUALFL`, `LEASTTXT`,
`GREATESTTXT`, `MAPCAT`, `TEXTSEQ`, `MINTXT`, and `MAXTXT` for each
subject from the collected text and a comparison text.

**Input:** demographics records carrying `RAWTXT` (collected text)
and `PEERTXT` (comparison text).

**Variables:**

- `UPPERTXT` is `RAWTXT` with American Standard Code for
  Information Interchange (ASCII) lowercase letters changed to
  uppercase; every other character is unchanged, and missing text
  gives the text `MISSING`.
- `LOWERTXT` is `RAWTXT` with ASCII uppercase letters changed to
  lowercase; every other character is unchanged, and missing text
  gives the text `MISSING`.
- `EQUALFL` is `Y` when `RAWTXT` and `PEERTXT` hold identical
  character sequences (same code points in the same order),
  including when both are missing; otherwise it is `N`.
- `LEASTTXT` is the earlier non-missing value of `RAWTXT` and
  `PEERTXT` by Unicode code-point order (called scalar order
  below), left blank when both are missing.
- `GREATESTTXT` is the later non-missing value of `RAWTXT` and
  `PEERTXT` by scalar order, left blank when both are missing.
- `MAPCAT` groups spellings that differ only in the case of ASCII
  letters: any casing of `abc` gives `ASCII` and `i` or `I` gives
  `ASCII_I`. Missing text gives the text `MISSING`, and any other
  text gives `OTHER`, so text holding a character outside ASCII
  never joins a group.
- `TEXTSEQ` numbers rows in scalar order of the collected text,
  breaking ties by subject, with missing text last.
- `MINTXT` is the earliest collected text in the study by scalar
  order, shown on every row.
- `MAXTXT` is the latest collected text in the study by scalar
  order, shown on every row.

**Note:** text is never normalized, so a composed and a decomposed
spelling of the same word (an accented letter stored as one
character, or as the base letter plus a combining accent) are
different values. The flag, the earlier and later values, the
sequence, and the minimum and maximum all follow scalar order.

**Standard:** ADaM | **Domain:** ADSL
