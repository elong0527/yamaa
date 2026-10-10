/* SAS language reference solution for the yamaa benchmark adam-adrs-best-response.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adrs_selection;
  length STUDYID USUBJID ADT RANDDY AVALC BORCAT BORPRI $1024;
  infile "/app/input/adrs_selection.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ASEQ ADT : $char1024. RANDDY : $char1024. AVALC : $char1024. BORCAT : $char1024. BORPRI : $char1024. BORSEQ;
run;

data adsl;
  length STUDYID USUBJID RANDDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RANDDT : $char1024.;
run;

proc sql; create table joined as select a.*,b.BORCAT as AVALC,b.ADT from adsl as a left join adrs_selection as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and b.BORSEQ=1; quit;
data result;
  length PARAMCD $8 PARAM $40;
  set joined; PARAMCD='BOR'; PARAM='Best Overall Response by Investigator';
  select(AVALC); when('CR') AVAL=1; when('PR') AVAL=2; when('SD') AVAL=3; when('NON-CR/NON-PD') AVAL=4; when('PD') AVAL=5; when('NE') AVAL=6; otherwise AVAL=.; end;
  if missing(AVALC) then ADT='';
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, RANDDT, AVALC, AVAL, ADT from result;
quit;
proc export data=final outfile="/app/output/adrs.csv" dbms=csv replace; run;
