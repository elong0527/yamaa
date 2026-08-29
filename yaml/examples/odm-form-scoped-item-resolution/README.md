# SDTM LB: resolve items within their collection form

This example uses a long-form ODM projection with five laboratory forms and a
`yamaa` specification to derive one record per reported result:

- `LBTESTCD`, `LBTEST`, and `LBCAT` identify the result and the form on which
  it was collected;
- `LBORRES` is the result reported on that form;
- `LBDTC` is the date from the same form, even though every form uses the same
  item identifier for its date. A form with no matching date keeps its result
  and has no `LBDTC`;
- `LBSEQ` numbers the forms in their declared order.

Form identity is part of the collection context. Dates from another form at
the same subject and visit cannot satisfy a missing date on the current form.
