# Carry Several Supplemental Qualifiers Onto Their Adverse Events

**Goal:** carry three record-level supplemental qualifiers of each adverse
event onto its analysis row: the location detail (`AEXSITE`), the suspected
cause (`AEXCAUS`) and the reaction size (`AEXSIZE`).

**Input:** one `spec.yaml` over adverse event records and the supplemental
qualifier records of the AE domain (SDTMIG section 8.4), each naming the
parent event by its sequence number, the qualifier name and its value.

**Variables:**

- `AETERM` is the reported term of the event, as collected.
- `AEXSITE` is the value of the event's `AEXSITE` qualifier; empty when the
  event has none.
- `AEXCAUS` is the value of the event's `AEXCAUS` qualifier; empty when the
  event has none.
- `AEXSIZE` is the value of the event's `AEXSIZE` qualifier as a number;
  empty when the event has none.

**Note:** one declaration names the three qualifiers, and each becomes a
field of the event's supplemental records. A subject-level qualifier, which
names no sequence number, and a qualifier of a sequence number no event
carries change nothing. A qualifier record whose name is none of the three
would stop the run rather than be dropped.
