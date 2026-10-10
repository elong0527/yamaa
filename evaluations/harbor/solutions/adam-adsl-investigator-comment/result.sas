/* opensas reference solution for the yamaa benchmark adam-adsl-investigator-comment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

/* The benchmark treats a spaces-only CSV field as a collected comment.
   The existing opensas CSV reader/exporter normalizes it to opensas missing.
   Track this compatibility gap in issue #1875 before benchmark execution. */
libname source '/app/input' access=readonly;
data result;
  length CMNT $32767 CMNTFL $1;
  set source.dm;
  CMNT=COMMENT;
  CMNTFL='N';
  if lengthn(COMMENT)>0 then CMNTFL='Y';
run;

proc sql;
  create table final as select STUDYID, USUBJID, CMNT, CMNTFL from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
