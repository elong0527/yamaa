/* opensas reference solution for the yamaa benchmark adam-adsl-treatment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID ACTARM $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ACTARM : $char1024.;
run;

data ex;
  length STUDYID USUBJID EXTRT EXSTDTC EXENDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024. EXDOSE EXSTDTC : $char1024. EXENDTC : $char1024.;
run;

data exposure;
  length startmoment endmoment $19;
  set ex;
  if upcase(EXTRT) in ('VITAMIN D3','PLACEBO');
  startmoment=EXSTDTC; endmoment=EXENDTC;
  if lengthn(EXSTDTC)=10 then startmoment=cats(EXSTDTC,'T00:00:00');
  else if lengthn(EXSTDTC)=16 then startmoment=cats(EXSTDTC,':00');
  if lengthn(EXENDTC)=10 then endmoment=cats(EXENDTC,'T23:59:59');
  else if lengthn(EXENDTC)=16 then endmoment=cats(EXENDTC,':00');
run;
proc sort data=exposure(where=(EXSTDTC ne '')) out=starts; by STUDYID USUBJID startmoment EXSEQ; run;
data firstex; set starts; by STUDYID USUBJID; if first.USUBJID; run;
proc sort data=exposure(where=(EXENDTC ne '')) out=ends; by STUDYID USUBJID descending endmoment; run;
data lastex; set ends; by STUDYID USUBJID; if first.USUBJID; run;
proc sql; create table joined as select a.*,b.EXTRT,b.EXSTDTC,b.startmoment,c.EXENDTC,c.endmoment from dm as a
  left join firstex as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join lastex as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID; quit;
data result;
  length TRT01A $1024 TRTSDT TRTEDT $10 TRTSDTM TRTEDTM $19 TRTSTMF TRTETMF SAFFL $1;
  set joined;
  TRT01A=upcase(coalescec(EXTRT,ACTARM,'NOT TREATED'));
  TRTSDT=''; TRTEDT=''; TRTSTMF=''; TRTETMF=''; TRTDURD=.;
  if lengthn(EXSTDTC)>=10 then TRTSDT=substr(EXSTDTC,1,10);
  if lengthn(EXENDTC)>=10 then TRTEDT=substr(EXENDTC,1,10);
  TRTSDTM=tranwrd(startmoment,'T',' '); TRTEDTM=tranwrd(endmoment,'T',' ');
  if lengthn(EXSTDTC)=10 then TRTSTMF='H';
  if lengthn(EXENDTC)=10 then TRTETMF='H';
  if not missing(TRTSDT) and not missing(TRTEDT) then TRTDURD=input(TRTEDT,yymmdd10.)-input(TRTSDT,yymmdd10.)+1;
  SAFFL='N'; if not missing(TRTSDT) then SAFFL='Y';
run;

proc sql;
  create table final as select STUDYID, USUBJID, TRT01A, TRTSDT, TRTSDTM, TRTSTMF, TRTEDT, TRTEDTM, TRTETMF, TRTDURD, SAFFL from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
