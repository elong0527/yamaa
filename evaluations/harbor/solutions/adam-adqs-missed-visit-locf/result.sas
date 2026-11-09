/* opensas reference solution for the yamaa benchmark adam-adqs-missed-visit-locf.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID EFFFL $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EFFFL : $char1024.;
run;

data qs;
  length STUDYID DOMAIN USUBJID QSTESTCD QSTEST QSSTAT VISIT $1024;
  infile "/app/input/qs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. QSSEQ QSTESTCD : $char1024. QSTEST : $char1024. QSSTRESN QSSTAT : $char1024. VISITNUM VISIT : $char1024.;
run;

proc sql;
  create table collected as select a.STUDYID,a.USUBJID,a.QSTESTCD as PARAMCD,a.VISIT as AVISIT,a.VISITNUM as AVISITN,a.QSSTRESN as AVAL,'' as DTYPE,a.QSSEQ,b.EFFFL from qs as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
  create table scored as select STUDYID,USUBJID,QSTESTCD,VISITNUM,QSSTRESN,QSSEQ from qs where not missing(QSSTRESN);
quit;
data schedule; length AVISIT PARAMCD $1024; set adsl; if EFFFL='Y'; PARAMCD='ACTOT'; do AVISITN=8,16,24; AVISIT=catx(' ','Week',strip(put(AVISITN,best32.))); output; end; run;
proc sql;
  create table missed as select a.* from schedule as a left join scored as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARAMCD=b.QSTESTCD and a.AVISITN=b.VISITNUM where missing(b.VISITNUM);
  create table candidates as select a.*,b.QSSTRESN as AVAL,b.QSSEQ,b.VISITNUM as sourcevisit from missed as a inner join scored as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARAMCD=b.QSTESTCD and b.VISITNUM>0 and b.VISITNUM<a.AVISITN;
quit;
proc sort data=candidates; by STUDYID USUBJID PARAMCD AVISITN descending sourcevisit; run;
data carried; length DTYPE $11; set candidates; by STUDYID USUBJID PARAMCD AVISITN; if first.AVISITN; DTYPE='LOCF'; run;
data result; length DTYPE $11; set collected carried; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, QSSEQ, EFFFL from result;
quit;
proc export data=final outfile="/app/output/adqs.csv" dbms=csv replace; run;
