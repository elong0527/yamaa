# Reject Word Separators on Title Case

**Goal:** derive the reported adverse event term in title case
(`AETERMTC`), with a comma and an open bracket starting a word as well as
a blank.

**Input:** collected adverse event (AE) records carrying the reported
term (`AETERM`), recorded in capitals.

**Variables:**

- `AETERMTC` would be the reported term with the first letter of each
  word in upper case and every other letter in lower case.

Title case starts a word at every run of letters, after any character
that is not a letter, so it has no list of word separators to choose;
only proper case does. The request names one beside title case, so it is
rejected before any data is read, and no artifact is accepted.

## How to fix

Decide which characters start a word. To start one only after the
characters you list, as the SAS `PROPCASE` function does, ask for proper
case and keep the list:

```yaml
str_case:
  source: AETERM
  to: proper
  delimiters: " /-(.,["
```

`PAIN,BACK` then gives `Pain,Back` and `INVESTIGATOR'S DECISION` gives
`Investigator's Decision`. To keep title case, drop `delimiters`; a word
then starts after every character that is not a letter, so
`INVESTIGATOR'S DECISION` gives `Investigator'S Decision`.
