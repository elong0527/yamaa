# Analysis Value Metadata by Parameter

**Goal:** carry `PARAMCD`, `PARAM`, `AVAL`, `AVALC` and `AVALU` into a
laboratory analysis dataset, and describe each parameter's analysis values
separately in the data-definition document.

**Input:** standardized laboratory results (SDTM LB), one record per subject
and test, with the result as text, as a number when it is one, and its unit.

**Variables:**

- `PARAMCD` is the laboratory test code: glucose, creatinine and the
  pregnancy test.
- `PARAM` names the parameter, with its unit for the numeric tests.
- `AVAL` is the numeric result; missing for the pregnancy test. Glucose is
  described with no decimals and creatinine with one, and both require a
  value on every record.
- `AVALC` is the result as text. For the pregnancy test it is required and is
  `NEGATIVE` or `POSITIVE`.
- `AVALU` is the result's unit; missing for the pregnancy test. For glucose
  and creatinine it is required and is `mg/dL` or `umol/L`.

**Note:** the data-definition document lists, for each analysis value, one
definition per parameter or group of parameters, with the condition on
`PARAMCD` that selects its records; a definition that names no description,
precision or codelist of its own keeps the variable's. The exchange file
`adlb.json` carries the variables alone, each as wide as its longest value.
