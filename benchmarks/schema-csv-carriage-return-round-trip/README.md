# Read Back a Comment Whose Line Break Is a Carriage Return and Line Feed

**Goal:** carry each collected comment (`COVAL`) from the comments listing
the first specification publishes into the value (`QVAL`) of a second
listing that reads it.

**Input:** `spec_co.yaml` reads `input/comments.csv`, three collected
comments, and publishes `co.csv`; `spec_suppco.yaml` reads that published
`co.csv`, linked to the specification that wrote it, and publishes
`suppco.csv`. The first comment breaks its line with a line feed, the second,
typed in a browser on Windows, with a carriage return and a line feed, and
the third has no line break.

**Variables:**

- `ORD` is the comment's ordinal, as collected.
- `COVAL` is the comment exactly as collected, its line break included.
- `QVAL` is that same comment, read back from the published listing.

**Note:** a comment holding a line break is written quoted, and a quoted
field keeps every character it holds, a carriage return included, so a
listing one specification publishes reads back unchanged in the next.
