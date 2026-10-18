# Column-Level Values on Rows Built From a Selection of Events

**Goal:** build one Analysis Dataset for Adverse Events (ADAE) row per
serious adverse event from a named selection of the collected events,
and complete every row's dictionary term, actual treatment, customized
query name and source domain from values stated once for each column.

**Input:** one `spec.yaml` over the collected adverse events
(`input/ae.csv`: study, subject, sequence, dictionary term and
seriousness), the subject-level file (`input/adsl.csv`: actual
treatment) and the sponsor's customized query (`input/cqterm.csv`: the
query name and the dictionary terms it collects).

**Variables:**

- `AEDECOD`: the dictionary term collected for the serious event that
  built the row.
- `TRTA`: the actual treatment of the event's subject, matched on study
  and subject.
- `CQ01NAM`: the name of the customized query that collects the event's
  dictionary term; blank when no query collects it.
- `SRCDOM`: `AE`, the domain every row comes from.

**Note:** the row template states only the keys; every other column is
stated once and evaluated for each row the template builds. A value read
through the selection's name comes from the very event that built the
row, a subject-level value is matched on the keys its file carries, and
the query is looked up by the term already completed on the row. A
non-serious event, or one whose seriousness was never recorded, leaves
no row.
