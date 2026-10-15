# Sentence Case and Title Case

**Goal:** derive `AVISIT` (sentence case) and `AVISIT_TITLE` (title
case) from the collected visit name, replacing a hand-written
visit-label mapping.

**Input:** lab records carrying `VISIT` (the collected visit name, e.g.
`WEEK 8`), copied to the output unchanged.

**Variables:**

- `AVISIT` is `VISIT` in sentence case: the first character becomes
  uppercase and every later letter lowercase, so `END OF TREATMENT`
  gives `End of treatment`.
- `AVISIT_TITLE` is `VISIT` in title case: each unbroken run of
  letters starts uppercase and continues lowercase, and any other
  character, such as a space or hyphen, ends a word: `END OF TREATMENT`
  gives `End Of Treatment` and `FOLLOW-UP` gives `Follow-Up`.

**Note:** only the plain English letters A to Z change case; every
other character is unchanged. The two forms agree whenever the visit
name holds one run of letters (`WEEK 8` gives `Week 8` both ways).
Rows are ordered by the collected visit name, then by subject.
