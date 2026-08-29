# ADaM ADRS: reject a best response restricted to pre-progression assessments

This example uses a subject list and the overall responses prepared for it to
attempt one best-response record per subject:

- `PDDT` is the date the subject first progressed, and is empty for a subject
  who never did;
- `AVAL` is the rank of the best response recorded up to and including that
  date, `1` for a complete response through `6` for an assessment that was not
  evaluable, and `AVALC` is the response that rank stands for. Every assessment
  counts for a subject who never progressed.

Which assessments are eligible depends on the subject's own progression date,
and a date derived for a subject cannot narrow the records that same subject
is summarized from. Selecting the best response over all assessments instead
would answer a different question, so the run must fail and no artifact is
accepted. The expected output records the intended result.
