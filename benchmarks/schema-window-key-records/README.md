# Numbered Results Each Keep Their Own Values

**Goal:** show that numbering each subject's results by study day (`LBSEQ`)
makes every collected laboratory result its own record, whose values all come
from that one result: the result as collected (`LBORRES`), the numeric result
(`LBSTRESN`), whether the test was done (`LBSTAT`), and whether the result is
above the upper limit of normal (`LBNRIND`).

**Input:** one `spec.yaml` over collected glucose results
(`input/lb_raw.csv`), each carrying the test code (`LBTESTCD`), the study day
(`LBDY`), the result as text (`RESULT`) or, when there is none, a character
result (`RESULTC`), the numeric result (`STRESN`) and the upper limit of
normal (`HIGH`). The `project.yaml` links each host's environment, whose
routine returns the numeric result it is given.

**Variables:**

- `LBSEQ` numbers each subject's results from 1 by study day; results of one
  day keep their collected order.
- `LBORRES` is the result as collected, or the character result when the
  result is blank.
- `LBSTRESN` is the numeric result, as the project's routine returns it, and
  blank when none was collected.
- `LBSTAT` is `NOT DONE` when no numeric result was collected, and blank
  otherwise.
- `LBNRIND` is `HIGH` above the upper limit and `NORMAL` otherwise, and blank
  when no numeric result was collected.

**Note:** a subject's two results of one day stay two records, and each
record's values come from its own result, never from the other result of
that day.
