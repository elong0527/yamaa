# Reported Terms in Proper Case

**Goal:** write each reported adverse event term in proper case, as the
SAS `PROPCASE` function writes it: once with its standard word separators
(`AETERMPC`) and once with a list that replaces the tab with a comma and
an open bracket (`AETERMPB`).

**Input:** collected adverse event (AE) records carrying the reported
term (`AETERM`), recorded in capitals.

**Variables:**

- `AETERMPC` is the reported term with every letter in lower case, then
  the first character and each character that follows a word separator
  in upper case. The separators are the blank, slash, hyphen, open
  parenthesis, period and tab, so `NON-CARDIAC CHEST PAIN` gives
  `Non-Cardiac Chest Pain`. A letter after any other character stays in
  lower case: `INVESTIGATOR'S DECISION` gives `Investigator's Decision`,
  `2ND DEGREE BURN` gives `2nd Degree Burn` and `PAIN,BACK` gives
  `Pain,back`. A missing term gives `Not Reported`.
- `AETERMPB` is the same with its own list of separators, which replaces
  the standard one: the blank, slash, hyphen, open parenthesis, period,
  comma and open bracket. So `PAIN,BACK` gives `Pain,Back` and
  `RASH [LEFT ARM]` gives `Rash [Left Arm]`, where `AETERMPC` gives
  `Rash [left Arm]`. A missing term gives `Not Reported`.

**Note:** only the plain English letters A to Z change case; every other
character is unchanged.
