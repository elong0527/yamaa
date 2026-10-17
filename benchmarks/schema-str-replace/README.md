# Clean Up Collected Text

**Goal:** tidy the spacing of each reported adverse event term and keep only
the digits of the sponsor-defined identifier.

**Input:** collected adverse events with one record for each event, carrying
the reported term (`AETERM`) and the sponsor-defined identifier (`AESPID`).

**Variables:**

- `AETERMC`: the reported term with each run of white space between words
  reduced to one space and none left at either end, so `  HEAD   ACHE `
  reads `HEAD ACHE`. A no-break space counts as white space; a term without
  extra white space is unchanged.
- `AESPDIG`: the digits of the identifier in their original order, so
  `AE-0-12` gives `012`; blank when the identifier is blank.

**Note:** an identifier with no digit at all gives an empty text value, not a
blank. The output file writes the empty text value as two quote characters
and leaves a blank empty.
