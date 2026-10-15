# Carry the last observed value across missing visits

**Goal:** fill each missing vital signs result with the last value
observed before it (last observation carried forward), in `AVAL`.

**Input:** one planned vital signs assessment per subject, parameter,
and analysis visit, holding either a collected value or a gap.

**Variables:**

- `AVISITN` identifies the planned analysis visit.
- `AVAL` is the collected value when there is one; otherwise it is the
  value from the closest earlier visit, in visit-number order, that has
  one, for the same subject and parameter. A gap before the first
  collected value stays missing, and zero is a collected value that is
  carried like any other.
