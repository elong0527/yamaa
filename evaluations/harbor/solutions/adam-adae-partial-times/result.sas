/* opensas reference solution for the yamaa benchmark adam-adae-partial-times.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AESTDTC AEENDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESTDTC : $char1024. AEENDTC : $char1024.;
run;

/* Complete each collected value by its length: the missing parts take their
   earliest (start) or latest (end) values, and the flag names the coarsest
   part supplied. A value without its day, or no value, gives no time. */
data result;
  length ASTDTM AENDTM $19 ASTTMF AENTMF $1;
  set ae;
  ASTDTM=''; ASTTMF=''; AENDTM=''; AENTMF='';
  ls=lengthn(AESTDTC);
  if ls=19 then ASTDTM=AESTDTC;
  else if ls=16 then do; ASTDTM=cats(AESTDTC, ':00'); ASTTMF='S'; end;
  else if ls=13 then do; ASTDTM=cats(AESTDTC, ':00:00'); ASTTMF='M'; end;
  else if ls=10 then do; ASTDTM=cats(AESTDTC, 'T00:00:00'); ASTTMF='H'; end;
  le=lengthn(AEENDTC);
  if le=19 then AENDTM=AEENDTC;
  else if le=16 then do; AENDTM=cats(AEENDTC, ':59'); AENTMF='S'; end;
  else if le=13 then do; AENDTM=cats(AEENDTC, ':59:59'); AENTMF='M'; end;
  else if le=10 then do; AENDTM=cats(AEENDTC, 'T23:59:59'); AENTMF='H'; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDTM, ASTTMF,
    AEENDTC, AENDTM, AENTMF from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
