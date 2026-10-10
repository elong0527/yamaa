/* opensas reference solution for the yamaa benchmark adam-adsl-investigator-comment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

/* Read DM line by line and split the fields here: a quoted comment can hold
   commas, doubled quotation marks, and line breaks, and a comment of only
   spaces keeps its spaces, so each field's length is counted as it is
   read. An empty field, quoted or not, has length zero. */
data dm;
  infile "/app/input/dm.csv" firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 COMMENT $1000 FIELD $1000 CH $1;
  retain STUDYID USUBJID COMMENT FIELD "" CMNTLEN FLEN NFIELD INQUOTE 0;
  keep STUDYID USUBJID COMMENT CMNTLEN;
  input;
  if lengthn(_infile_) = 0 and not INQUOTE then delete;
  RECLEN = lengthc(_infile_);
  do I = 1 to RECLEN;
    CH = substr(_infile_, I, 1);
    if INQUOTE and CH = '"' then do;
      /* A doubled quotation mark inside quotes is one mark of the value. */
      if I < RECLEN and substr(_infile_, I + 1, 1) = '"' then do;
        link addchar;
        I + 1;
      end;
      else INQUOTE = 0;
    end;
    else if not INQUOTE and CH = '"' then INQUOTE = 1;
    else if not INQUOTE and CH = "," then link endfield;
    else link addchar;
  end;

  /* A line break inside quotes is part of the comment; anywhere else it
     ends the record. */
  if INQUOTE then do;
    CH = '0A'x;
    link addchar;
  end;
  else do;
    link endfield;
    output;
    call missing(STUDYID, USUBJID, COMMENT);
    CMNTLEN = 0;
    NFIELD = 0;
  end;
  return;

/* Append one character, a space included, to the field being read. */
addchar:
  if FLEN = 0 then FIELD = CH;
  else FIELD = substr(FIELD, 1, FLEN) || CH;
  FLEN + 1;
  return;

/* Store the field just read under its column. */
endfield:
  NFIELD + 1;
  if NFIELD = 1 then STUDYID = FIELD;
  else if NFIELD = 2 then USUBJID = FIELD;
  else if NFIELD = 3 then do;
    COMMENT = FIELD;
    CMNTLEN = FLEN;
  end;
  FIELD = "";
  FLEN = 0;
  return;
run;

/* CMNT is the comment as recorded, CMNTFL Y when one was collected. */
proc sql;
  create table adsl as
  select STUDYID, USUBJID,
         case when CMNTLEN > 0 then COMMENT else "" end as CMNT length=1000,
         case when CMNTLEN > 0 then "Y" else "N" end as CMNTFL length=1,
         CMNTLEN
  from dm
  order by STUDYID, USUBJID;
quit;

/* PROC EXPORT would drop the spaces of a comment of only spaces, so write
   the file directly: the comment quoted, with its quotation marks doubled,
   at its recorded length. */
data _null_;
  set adsl;
  file "/app/output/adsl.csv";
  length QUOTED $2000 OUTLINE $2200;
  if _n_ = 1 then put "STUDYID,USUBJID,CMNT,CMNTFL";
  if CMNTLEN = 0 then
    OUTLINE = trim(STUDYID) || "," || trim(USUBJID) || ",," || CMNTFL;
  else do;
    QUOTED = tranwrd(CMNT, '"', '""');
    QLEN = CMNTLEN + countc(CMNT, '"');
    OUTLINE = trim(STUDYID) || "," || trim(USUBJID) || ',"'
              || substr(QUOTED, 1, QLEN) || '",' || CMNTFL;
  end;
  put OUTLINE;
run;
