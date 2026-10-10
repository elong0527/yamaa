/* opensas reference solution for the yamaa benchmark adam-adsl-flag-chain.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID RANDDT $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AGE RANDDT : $char1024.;
run;

data ex;
  length STUDYID USUBJID EXSTDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXSTDTC : $char1024.;
run;

proc sql;
  create table firstex as select STUDYID,USUBJID,min(EXSTDTC) as TRTSDT from ex where not missing(EXSTDTC) group by STUDYID,USUBJID;
  create table joined as select a.*,b.TRTSDT from dm as a left join firstex as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;
proc sort data=joined; by STUDYID AGE USUBJID; run;
data result;
  length POPFL SAFFL ITTFL $1 AGEGR1 $4;
  set joined; by STUDYID;
  if first.STUDYID then AGERNK=0; AGERNK+1;
  SAFFL='N'; ITTFL='N'; POPFL='N';
  if not missing(TRTSDT) then SAFFL='Y';
  if not missing(RANDDT) then ITTFL='Y';
  if SAFFL='Y' and ITTFL='Y' then POPFL='Y';
  if AGE<65 then AGEGR1='<65'; else AGEGR1='>=65';
run;

proc sql;
  create table final as select POPFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, AGERNK, STUDYID, USUBJID, AGE from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
