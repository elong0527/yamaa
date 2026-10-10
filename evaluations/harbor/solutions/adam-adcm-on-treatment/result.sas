/* opensas reference solution for the yamaa benchmark adam-adcm-on-treatment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDT TRTEDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024. TRTEDT : $char1024.;
run;

data cm;
  length STUDYID USUBJID CMTRT CMSTDTC CMENDTC $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMSTDTC : $char1024. CMENDTC : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDT,b.TRTEDT from cm as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result;
  length ASTDT AENDT $10 ONTRTFL $1;
  set joined; ASTDT=CMSTDTC; AENDT=CMENDTC;
  if not missing(TRTSDT) and (missing(AENDT) or AENDT>=TRTSDT) and (missing(TRTEDT) or missing(ASTDT) or ASTDT<=TRTEDT) then ONTRTFL='Y';
run;

proc sql;
  create table final as select STUDYID, USUBJID, CMSEQ, CMTRT, ASTDT, AENDT, TRTSDT, TRTEDT, ONTRTFL from result;
quit;
proc export data=final outfile="/app/output/adcm.csv" dbms=csv replace; run;
