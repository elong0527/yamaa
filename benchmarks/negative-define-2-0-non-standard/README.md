# Reject an older submission format for sponsor-defined records

**Goal:** carry examination sequence (`ZESEQ`) and name (`ZETERM`) in a
sponsor-defined domain and request its submission metadata.

**Input:** an electronic case report form export lists each subject's
examination records with a sequence number and the recorded examination name.

**Variables:**

- `ZESEQ` identifies the examination record within the subject.
- `ZETERM` carries the examination name as recorded.

**Note:** the dataset builds, but Define-XML 2.0 cannot represent its declared
sponsor-defined status. The submission is refused before its metadata or
transport file is published.

## How to fix

Use a submission format that preserves the sponsor-defined status, without
claiming that this custom dataset follows an implementation-guide definition:

```yaml
sdtm:
  define_version: '2.1.11'
```

Keep `submission.non_standard: true` on the custom dataset. The section's
transport request and the collected name's annotated-form reference remain.
