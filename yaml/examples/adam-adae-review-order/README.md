# ADaM ADAE: present a subject's events in medical-review order

This example uses collected adverse events and a `yamaa` specification to
derive one record per event, in the order a medical reviewer reads them:

- `AETERM` is the reported term for the event;
- `ASTDT` is the date the event began, and is empty when no onset date was
  collected;
- `ASEV` is the reported severity, and is empty when severity was not
  reported.

A subject's events come worst first, and the events sharing a severity come by
onset date, earliest first. An event whose onset was not collected precedes the
dated events of its severity, so the record that still needs a date is read
rather than overlooked. An event whose severity was not reported follows every
reported severity, because an absent severity is not a mild one. Two events a
reviewer cannot tell apart -- the same severity on the same date -- stay in
collection order.
