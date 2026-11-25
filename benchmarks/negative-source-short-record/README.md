# Reject a Column-Aligned Line Cut Short

**Goal:** list each subject's height (`HEIGHT`), weight (`WEIGHT`) and visit
date (`VISITDT`) from a legacy vital-signs transfer whose fields sit at fixed
character positions.

**Input:** a plain-text transfer with the subject identifier in positions 1
to 4, the height in 5 to 7, the weight in 8 to 14 and the visit date in 15
to 24, after a column ruler line. One subject's visit date was blank, and
the line was saved without the spaces that filled it, so it ends after the
weight.

**Variables:**

- `HEIGHT` would be the height in centimetres.
- `WEIGHT` would be the weight in kilograms.
- `VISITDT` would be the date of the visit.

A line that ends before the last field's positions cannot say whether the
missing characters were blank or lost in transfer, so the run is rejected
while reading the transfer and no artifact is accepted.

## How to fix

Ask for the transfer as it was written, every line as long as its layout,
with spaces filling a blank value, so the last subject's line runs on with
ten spaces to position 24.

Where the producer cannot resend it, pad the short line with spaces to the
last field's last position before reading it, and record that the padding
was added.
