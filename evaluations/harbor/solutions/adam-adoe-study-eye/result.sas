/* SAS language reference solution for the yamaa benchmark adam-adoe-study-eye.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID STUDYEYE $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. STUDYEYE : $char1024.;
run;

data oe;
  length STUDYID USUBJID OETESTCD OELAT $1024;
  infile "/app/input/oe.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. OESEQ OETESTCD : $char1024. OELAT : $char1024. OESTRESN;
run;

proc sql;
  create table result as
  select a.*,a.OETESTCD as PARAMCD,a.OESTRESN as AVAL,case when missing(a.OELAT) or missing(b.STUDYEYE) then '' when a.OELAT='BILATERAL' then 'Both Eyes' when b.STUDYEYE='BILATERAL' or a.OELAT=b.STUDYEYE then 'Study Eye' else 'Fellow Eye' end as AFEYE from oe as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, OESEQ, PARAMCD, OELAT, AVAL, AFEYE from result;
quit;
proc export data=final outfile="/app/output/adoe.csv" dbms=csv replace; run;
