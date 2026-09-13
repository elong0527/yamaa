# Preserve and compare international text

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-portable-text.html)

**Goal:** derive `UPPERTXT`, `LOWERTXT`, `EQUALFL`, `LEASTTXT`,
`GREATESTTXT`, `MAPCAT`, `TEXTSEQ`, `MINTXT`, and `MAXTXT` for each
subject from the collected text and a comparison text.

**Input:** demographics records carrying `RAWTXT` (collected text)
and `PEERTXT` (comparison text).

**Variables:**

- `RAWTXT` is the collected text, carried through unchanged.
- `UPPERTXT` is `RAWTXT` with American Standard Code for
  Information Interchange (ASCII) lowercase letters changed to
  uppercase; every other character is unchanged (comparison is by
  Unicode code-point order, called scalar order below), and missing
  text gives the text `MISSING`.
- `LOWERTXT` is `RAWTXT` with ASCII uppercase letters changed to
  lowercase; every other character is unchanged, and missing text
  gives the text `MISSING`.
- `EQUALFL` is `Y` when `RAWTXT` and `PEERTXT` hold identical
  character sequences (same code points in the same order),
  including when both are missing; otherwise it is `N`.
- `LEASTTXT` is the earlier non-missing value of `RAWTXT` and
  `PEERTXT` by scalar order, left blank when both are missing.
- `GREATESTTXT` is the later non-missing value of `RAWTXT` and
  `PEERTXT` by scalar order, left blank when both are missing.
- `MAPCAT` groups ASCII spelling variants without regard to case:
  `ABC` gives `ASCII` and `I` gives `ASCII_I`, missing text gives
  the text `MISSING`, and any other text gives `OTHER`, so text
  outside ASCII never joins a group.
- `TEXTSEQ` numbers rows in scalar order of the collected text,
  breaking ties by subject, with missing text last.
- `MINTXT` is the earliest collected text in the study by scalar
  order, shown on every row.
- `MAXTXT` is the latest collected text in the study by scalar
  order, shown on every row.

**Note:** every comparison uses scalar order, so composed and
decomposed spellings of the same word compare as different values
and the flag, earlier, later, sequence, minimum, and maximum values
all follow that one order.

**Standard:** ADaM | **Domain:** ADSL
