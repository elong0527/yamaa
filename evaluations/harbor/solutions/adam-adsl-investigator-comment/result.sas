/* opensas reference solution for the yamaa benchmark adam-adsl-investigator-comment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID $40;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. USUBJID : $char40.;
run;

/* Read CO line by line and split the fields here: a quoted comment can hold
   commas, doubled quotation marks, and line breaks, so each field's length
   is counted as it is read. An empty field, quoted or not, has length
   zero. */
data co;
  infile "/app/input/co.csv" firstobs=2 truncover lrecl=32767;
  length USUBJID $40 COVAL COVAL1 FIELD $400 CH $1;
  retain USUBJID COVAL COVAL1 FIELD "" COSEQ VALLEN VAL1LEN FLEN NFIELD INQUOTE 0;
  keep USUBJID COSEQ COVAL COVAL1 VALLEN VAL1LEN;
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
    call missing(USUBJID, COVAL, COVAL1);
    COSEQ = .;
    VALLEN = 0;
    VAL1LEN = 0;
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
  if NFIELD = 4 then USUBJID = FIELD;
  else if NFIELD = 5 then COSEQ = input(FIELD, best32.);
  else if NFIELD = 6 then do;
    COVAL = FIELD;
    VALLEN = FLEN;
  end;
  else if NFIELD = 7 then do;
    COVAL1 = FIELD;
    VAL1LEN = FLEN;
  end;
  FIELD = "";
  FLEN = 0;
  return;
run;

/* Each comment is COVAL, then COVAL1 after one space when it continues
   there; a subject's comments join in COSEQ order with "; ". */
proc sort data=co; by USUBJID COSEQ; run;
data comments;
  set co;
  by USUBJID;
  length TEXT $1000 CMNT $2000;
  retain CMNT "" CMNTLEN 0;
  keep USUBJID CMNT CMNTLEN;
  TEXT = substr(COVAL, 1, VALLEN);
  TLEN = VALLEN;
  if VAL1LEN > 0 then do;
    TEXT = substr(COVAL, 1, VALLEN) || " " || substr(COVAL1, 1, VAL1LEN);
    TLEN = VALLEN + 1 + VAL1LEN;
  end;
  if first.USUBJID then do;
    CMNT = TEXT;
    CMNTLEN = TLEN;
  end;
  else do;
    CMNT = substr(CMNT, 1, CMNTLEN) || "; " || substr(TEXT, 1, TLEN);
    CMNTLEN = CMNTLEN + 2 + TLEN;
  end;
  if last.USUBJID then output;
run;

proc sql;
  create table adsl as
  select d.STUDYID, d.USUBJID,
         case when c.CMNTLEN > 0 then c.CMNT else "" end as CMNT length=2000,
         case when c.CMNTLEN > 0 then "Y" else "N" end as CMNTFL length=1,
         coalesce(c.CMNTLEN, 0) as CMNTLEN
  from dm as d left join comments as c on d.USUBJID = c.USUBJID
  order by d.STUDYID, d.USUBJID;
quit;

/* Write the file directly: the comment quoted, with its quotation marks
   doubled, at its recorded length. */
data _null_;
  set adsl;
  file "/app/output/adsl.csv";
  length QUOTED $4000 OUTLINE $4200;
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
