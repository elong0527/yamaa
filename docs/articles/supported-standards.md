# Supported standards

yamaa generates clinical datasets, Define-XML metadata and SAS Transport
version 5 files for the implementation guides below. A study declares its
guide explicitly and supplies the dataset content, terminology and analysis
rules that follow that guide.

## Clinical guides

| Model | Implementation guide versions | Minimum supported pair |
| --- | --- | --- |
| SDTM | 3.3, 3.4 | SDTM 1.7 / SDTMIG 3.3 |
| ADaM | 1.1, 1.2, 1.3 | ADaM 2.1 / ADaMIG 1.1 |

SDTMIG 3.4 uses SDTM 2.0. Every listed ADaMIG version uses ADaM 2.1.
The minimum-version badges refer to this finite list of published guides;
a future version needs its own compatibility checks before joining it.

For example, a project can select the minimum guides independently:

```yaml
yamaa: "1.0"
type: project
study:
  id: STUDY01
  name: STUDY01
  description: Clinical study
  protocol_name: STUDY01
sdtm:
  specs: [sdtm/dm.yaml]
  standards:
    - {name: SDTMIG, version: "3.3", status: Final}
  define: sdtm/define.xml
  transport: xpt
adam:
  specs: [adam/adsl.yaml]
  standards:
    - {name: ADaMIG, version: "1.1", status: Final}
  define: adam/define.xml
  transport: xpt
```

Define-XML and controlled-terminology releases are separate choices. An
older implementation guide can use Define-XML 2.1; selecting SDTMIG 3.3 or
ADaMIG 1.1 does not require Define-XML 2.0 or a terminology release from the
guide's publication year. The project codelist sources declare the
terminology releases the study actually uses.

## Compatibility coverage

The independently authored benchmarks compare derived rows, complete
Define-XML documents and XPT v5 bytes. The shared qualification readers run
them through the Python and R hosts. XPT expectations come from an
independent encoder of the public transport layout and are also read with
an independent reader. The generated XML is checked against CDISC's XSD.

| Guide | Benchmark | What it exercises |
| --- | --- | --- |
| SDTMIG 3.3 | [Demographics](https://elong0527.github.io/yamaa/benchmark/schema-sdtmig33-dm.html) | Required and expected metadata, missing dates, numeric and text transport |
| SDTMIG 3.3 | [Subject visits](https://elong0527.github.io/yamaa/benchmark/schema-sdtmig33-sv.html) | The 3.3 visit layout, fractional visit numbers, a visit spanning days and an unplanned visit |
| SDTMIG 3.3 | [Morphology](https://elong0527.github.io/yamaa/benchmark/schema-sdtmig33-mo.html) | The MO domain retained in 3.3, collected and standardized findings, and timing roles |
| ADaMIG 1.1 | [Subject-level analysis](https://elong0527.github.io/yamaa/benchmark/schema-adamig11-adsl.html) | Subject keys, treatment metadata, numeric dates and datetimes, and display formats |
| ADaMIG 1.1 | [Laboratory analysis](https://elong0527.github.io/yamaa/benchmark/schema-adamig11-adlb.html) | BDS parameters, subject-level treatment joins, relative days, repeated baseline, change and absent baseline |
| ADaMIG 1.1 | [Adverse-event analysis](https://elong0527.github.io/yamaa/benchmark/schema-adamig11-adae.html) | Occurrence data, partial onset times, imputation flags and numeric analysis date/time transport |
| ADaMIG 1.2 | [Subject-level analysis](https://elong0527.github.io/yamaa/benchmark/schema-adamig12-adsl.html) | An explicit intermediate guide declaration with the same date/time transport contract |

The existing submission benchmarks retain SDTMIG 3.4 and ADaMIG 1.3
coverage. The new examples select the older content directly: MO remains a
3.3 domain, and the 3.3 SV example carries no 3.4-only visit variables.

This is generation and export compatibility. The engine checks the study's
declared metadata and verifications; it does not contain the full catalog
of every guide's domain, variable and business rules. Study teams validate
their complete submission against the selected guide and applicable FDA
rules. Changing the guide declaration alone does not convert newer content
into a conforming older dataset.

## FDA submission use

The minimum pairs above are supported clinical options in FDA Data
Standards Catalog v11.0. FDA eligibility depends on the study start date,
submission type and reviewing center. See the current
[FDA catalog](https://www.fda.gov/regulatory-information/search-fda-guidance-documents/data-standards-catalog)
and [study-data requirements](https://www.fda.gov/industry/study-data-standards-resources/study-data-submission-cder-and-cber)
when planning a submission.

CDISC's [SDTMIG 3.3](https://www.cdisc.org/standards/foundational/sdtmig/sdtmig-v3-3)
and [ADaM compatibility table](https://www.cdisc.org/standards/foundational/adam)
identify the underlying models and applicable companion documents. Their
errata and the selected occurrence-data guide remain part of the study's
standards assessment.

Historical metadata reproduction remains available for older received
declarations. Its literal metadata does not establish generation support
for every version string it can record.
