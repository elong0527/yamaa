/* opensas reference solution for the yamaa benchmark sdtm-co-long-comments.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

/* A quoted comment can hold commas; DSD reads it as one field. */
data co_raw;
  length STUDYID USUBJID $40 COMMENT $1000;
  infile "/app/input/co_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. USUBJID : $char40. COSEQ COMMENT : $char1000.;
run;

/* A blank comment gives no record. The comment is cut into pieces of 200
   characters at exact positions: COVAL from character 1, COVAL1 from 201
   and COVAL2 from 401, each shorter when the comment ends inside it and
   empty when the comment ends before it begins. */
data result;
  set co_raw;
  length DOMAIN $2 COVAL COVAL1 COVAL2 $200;
  if missing(COMMENT) then delete;
  DOMAIN = 'CO';
  LEN = lengthn(COMMENT);
  COVAL = substr(COMMENT, 1, min(LEN, 200));
  COVAL1 = '';
  COVAL2 = '';
  if LEN > 200 then COVAL1 = substr(COMMENT, 201, min(LEN - 200, 200));
  if LEN > 400 then COVAL2 = substr(COMMENT, 401, min(LEN - 400, 200));
run;

proc sql;
  create table final as
  select DOMAIN, STUDYID, USUBJID, COSEQ, COVAL, COVAL1, COVAL2 from result;
quit;
proc export data=final outfile="/app/output/co.csv" dbms=csv replace; run;
