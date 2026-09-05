# ADaM ADSL: preserve and compare international text predictably

This example uses sample DM text and produces one row per subject:

- `RAWTXT` preserves the collected scalar sequence;
- `UPPERTXT` changes ASCII lowercase letters to uppercase and leaves every
  other scalar unchanged;
- `LOWERTXT` changes ASCII uppercase letters to lowercase and leaves every
  other scalar unchanged;
- `EQUALFL` is `Y` only for identical scalar sequences, including when both
  values are missing;
- `LEASTTXT` is the earlier non-missing value by scalar order;
- `GREATESTTXT` is the later non-missing value by scalar order;
- `MAPCAT` groups ASCII spelling variants and does not fold non-ASCII text;
- `TEXTSEQ` numbers values in scalar order with missing last;
- `MINTXT` is the earliest collected text in the study;
- `MAXTXT` is the latest collected text in the study.

The data include ASCII, an accent in composed and decomposed forms, U+00DF,
U+0130, U+0131, a supplementary-plane scalar, and missing text.
