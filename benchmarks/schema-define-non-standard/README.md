# Submit sponsor-defined examination records

**Goal:** carry examination sequence (`ZESEQ`) and name (`ZETERM`) in a
sponsor-defined domain, with its submission metadata and transport file.

**Input:** an electronic case report form export lists each subject's
examination records with a sequence number and the recorded examination name.

**Variables:**

- `ZESEQ` identifies the examination record within the subject.
- `ZETERM` carries the examination name as recorded.

**Note:** the examination dataset is explicitly marked sponsor-defined.
Its Define-XML 2.1 metadata records that status rather than claiming an
implementation-guide definition, and links its SAS transport file. The
collected examination name cites page 1 of the annotated case report form.
