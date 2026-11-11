# Assign Each Vital Sign's Trial Period

**Goal:** carry each collected vital-signs result with its test, result,
and collection date-time, and add the trial period from the subject's
own elements: `VSTESTCD`, `VSORRES`, `VSDTC`, and `EPOCH`.

**Input:** collected vital-signs rows with test, result, and collection
date-time; and the subject's elements with the start and end of each
element.

**Variables:**

- `VSDTC` is the collection date-time, carried over unchanged; missing
  when no date was collected.
- `EPOCH` is the trial period of the element in progress at the
  collection date-time: the element whose start is on or before the
  collection and whose end is on or after it. A reading collected as a
  date alone is compared by date: the element whose start date is on or
  before that date and whose end date is on or after it. It is missing
  when the reading has no date or only a partial one, or falls outside
  every element, such as before the first element starts.

**Note:** the first-dose day shows why a date-time matters: a pre-dose
reading on that day belongs to screening and a post-dose reading to
treatment. A date-only reading on that day has no time to place it, and
no time is assumed; it matches both elements by date. When a reading
matches two elements, at a shared boundary moment or on a shared
boundary date, it belongs to the later element.
