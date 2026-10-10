/* opensas reference solution for the yamaa benchmark adam-adae-severity.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AEDECOD $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEDECOD : $char1024.;
run;

data supp;
  length STUDYID USUBJID AESEV $1024;
  infile "/app/input/supp.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AESEV : $char1024.;
run;

proc sql;
  create table result as
  select a.*,b.AESEV from ae as a left join supp as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEDECOD, AESEV from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
