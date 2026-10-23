# Give Each Adverse Event the Dose Nearest Its Start

**Goal:** give each adverse event the study-drug dose given nearest in time
to the event's start: the dose's sequence number (`EXSEQ`), its start date
and time (`EXSTDTM`) and its amount (`EXDOSE`).

**Input:** one `spec.yaml` over adverse events, each with its sequence
number and start date and time, and exposure records, each with its
sequence number, collected start date or date and time, and dose.

**Variables:**

- `ASTDTM`: the event's start date and time; empty when none was recorded.
- `EXSEQ`: the sequence number of the subject's dose nearest in time to the
  event's start, before or after it. Empty when the event has no start, or
  when no dose of the subject has a start to measure from.
- `EXSTDTM`: the start of that dose. A dose collected with a date alone
  starts at midnight of that day.
- `EXDOSE`: the amount of that dose.

**Note:** a dose with no collected start, or one collected only to the month,
has no time to measure from and is never chosen, even when it was recorded
first; it is passed over rather than taken as the nearest.
