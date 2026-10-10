/* opensas reference solution for the yamaa benchmark adam-adae-severity-rank.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AESEV $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESEV : $char1024.;
run;

data graded;
  length ASEV $20;
  set ae; ASEV=AESEV;
  select(ASEV); when('MILD') ASEVN=1; when('MODERATE') ASEVN=2; when('SEVERE') ASEVN=3; when('LIFE-THREATENING') ASEVN=4; otherwise ASEVN=.; end;
run;
proc sort data=graded; by STUDYID USUBJID descending ASEVN AESEQ; run;
data result;
  set graded; by STUDYID USUBJID descending ASEVN;
  retain count SEVRANK SEVLVL;
  if first.USUBJID then do; count=0; SEVRANK=0; SEVLVL=0; end;
  count+1;
  if first.ASEVN then do; SEVRANK=count; SEVLVL+1; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, ASEV, ASEVN, SEVRANK, SEVLVL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
