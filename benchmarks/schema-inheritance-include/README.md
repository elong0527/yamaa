# One Fragment on Two Templates

**Goal:** demonstrate one reusable fragment adding the analysis duration
`ADURN` to both an adverse event and a concomitant medication dataset, with a
second fragment adding the severity code `ASEVN` to adverse events only.

**Input:** six spec files:

- `spec_ae.yaml` and `spec_cm.yaml` are the templates: each reads its own
  records and gives the subject, the record's sequence number, `AESEQ` or
  `CMSEQ`, and its analysis start and end dates, `ASTDT` and `AENDT`;
- `spec_duration.yaml` derives the duration from those two dates and names no
  template of its own;
- `spec_severity.yaml` codes the reported severity of an adverse event;
- `spec_adae.yaml` is the adverse event entry: it names `spec_ae.yaml` as its
  parent, lists both fragments under `include`, and declares the complete
  output;
- `spec_adcm.yaml` is the medication entry: it names `spec_cm.yaml` as its
  parent and lists the duration fragment alone.

**Variables:**

- `ADURN` is the number of days from `ASTDT` to `AENDT`, counting both the
  first and the last day, so an event that starts and ends on one day lasts 1;
  it is missing while the end date is not known;
- `ASEVN` (adverse events only) is 1 for a mild, 2 for a moderate and 3 for a
  severe event.

**Note:** each entry composes its parent first, then the fragments it lists
in order, then its own fields, so a fragment adds to whichever template
includes it and the entry has the last word. The one duration file serves
both datasets: nothing in it names a template.
