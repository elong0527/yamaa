/* SAS language reference solution for the yamaa benchmark adam-adsl-demographics.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID SEX RACE $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. SEX : $char1024. RACE : $char1024. AGE;
run;

data result;
  length AGEGR1 $7;
  set dm;
  if missing(SEX) then SEX='U';
  select(SEX); when('M') SEXN=1; when('F') SEXN=2; otherwise SEXN=0; end;
  select(RACE);
    when('') RACEN=.; when('WHITE') RACEN=1;
    when('BLACK OR AFRICAN AMERICAN') RACEN=2;
    when('ASIAN') RACEN=3; when('MULTIPLE') RACEN=4; otherwise RACEN=99;
  end;
  if AGE ne int(AGE) then AGE=.;
  if missing(AGE) then AGEGR1='UNKNOWN';
  else if AGE<18 then AGEGR1='<18';
  else if AGE<65 then AGEGR1='18-64'; else AGEGR1='>=65';
run;

proc sql;
  create table final as select STUDYID, USUBJID, SEX, SEXN, RACE, RACEN, AGE, AGEGR1 from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
