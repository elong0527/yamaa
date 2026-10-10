/* SAS language reference solution for the yamaa benchmark adam-advs-bsa.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data advs;
  length STUDYID USUBJID PARAMCD PARAM VISIT $1024;
  infile "/app/input/advs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. PARAM : $char1024. AVAL VISIT : $char1024.;
run;

proc sql;
  create table computed as select a.STUDYID,a.USUBJID,'BSA' as PARAMCD,'Body Surface Area (m^2)' as PARAM,sqrt(a.AVAL*b.AVAL/3600) as AVAL,a.VISIT,'CALCULATION' as DTYPE from advs as a inner join advs as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.VISIT=b.VISIT
  where a.PARAMCD='HEIGHT' and b.PARAMCD='WEIGHT' and not missing(a.AVAL) and not missing(b.AVAL);
quit;
data collected; length DTYPE $11; set advs; run;
data result; set collected computed; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
