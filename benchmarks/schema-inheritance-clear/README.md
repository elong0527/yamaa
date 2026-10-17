# Inherited Entry Removal

**Goal:** demonstrate a study level removing one entry from a translation it
inherits, keeping the rest, to derive `DCSREAS`.

**Input:** two spec files, read root-first:

- `spec_organization.yaml` translates each collected discontinuation reason
  code, AE, LTFU, WBS and PD, into its reason, and reports any other code as
  OTHER;
- `spec_study.yaml` is the entry: it names the organization level as its
  parent, removes only the translation of PD with `null`, and declares the
  complete output.

**Variables:**

- `DCSREAS` is ADVERSE EVENT, LOST TO FOLLOW-UP or WITHDRAWAL BY SUBJECT from
  the code; a PD code is OTHER, because progressive disease is not a reason
  this study recognizes.

**Note:** a `null` written by a later level removes the value it inherits at
that one place and nothing else. The other three translations, and OTHER for
a code without a translation, still come from the organization level.
