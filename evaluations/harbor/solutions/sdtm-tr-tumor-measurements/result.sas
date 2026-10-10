/* SAS language reference solution for the yamaa benchmark sdtm-tr-tumor-measurements.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data tr_raw;
  length STUDYID USUBJID TRLNKID TRTESTCD TRTEST TRORRES TRORRESU TRSTAT TRMETHOD TREVAL TRDTC $1024;
  infile "/app/input/tr_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRSEQ TRLNKID : $char1024. TRTESTCD : $char1024. TRTEST : $char1024. TRORRES : $char1024. TRORRESU : $char1024. TRSTAT : $char1024. TRMETHOD : $char1024. TREVAL : $char1024. VISITNUM TRDTC : $char1024.;
run;

data result; set tr_raw; length DOMAIN $2 TRSTRESC TRSTRESU $1024; DOMAIN='TR'; TRSTRESN=.; TRSTRESC=''; TRSTRESU='';
if TRSTAT='NOT DONE' or missing(TRORRES) then do; TRSTAT='NOT DONE'; TRORRES=''; TRORRESU=''; TRDTC=''; end;
else if TRTESTCD='TUMSTATE' then TRSTRESC=TRORRES;
else do; if TRORRES='TOO SMALL TO MEASURE' then TRSTRESN=5; else do; TRSTRESN=input(TRORRES,?? best32.); if TRORRESU='cm' then TRSTRESN=TRSTRESN*10; end;
if not missing(TRSTRESN) then do; TRSTRESC=strip(put(TRSTRESN,best32.)); TRSTRESU='mm'; end; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, TRSEQ, TRLNKID, TRTESTCD, TRTEST, TRORRES, TRORRESU, TRSTRESC, TRSTRESN, TRSTRESU, TRSTAT, TRMETHOD, TREVAL, VISITNUM, TRDTC from result;
quit;
proc export data=final outfile="/app/output/tr.csv" dbms=csv replace; run;
