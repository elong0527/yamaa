/* opensas reference solution for the yamaa benchmark adam-adae-serious-events.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AEDECOD AESER $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEDECOD : $char1024. AESER : $char1024.;
run;

proc sql;
  create table result as
  select * from ae where AESER='Y';
quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEDECOD, AESER from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
