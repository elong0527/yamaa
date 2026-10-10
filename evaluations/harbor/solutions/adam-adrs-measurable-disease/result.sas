/* SAS language reference solution for the yamaa benchmark adam-adrs-measurable-disease.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

data tu;
  length STUDYID USUBJID TUTESTCD VISIT TUSTRESC $1024;
  infile "/app/input/tu.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TUSEQ TUTESTCD : $char1024. VISIT : $char1024. TUSTRESC : $char1024.;
run;

proc sql;
  create table counts as select STUDYID,USUBJID,count(*) as matches from tu where TUTESTCD='TUMIDENT' and VISIT='SCREENING' and TUSTRESC='TARGET' group by STUDYID,USUBJID;
  create table joined as select a.*,b.matches from adsl as a left join counts as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;
data result;
  length PARAMCD $8 PARAM $40 AVALC $1;
  set joined; PARAMCD='MDIS'; PARAM='Measurable Disease at Baseline';
  AVALC='N'; AVAL=0; if matches>0 then do; AVALC='Y'; AVAL=1; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVALC, AVAL from result;
quit;
proc export data=final outfile="/app/output/adrs.csv" dbms=csv replace; run;
