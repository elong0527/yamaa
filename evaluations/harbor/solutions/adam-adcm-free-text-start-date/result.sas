/* opensas reference solution for the yamaa benchmark adam-adcm-free-text-start-date.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data cm;
  length STUDYID USUBJID CMTRT CMSTDAT $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMSTDAT : $char1024.;
run;

/* The start is read in five spellings: YYYY-MM-DD, DDMONYYYY and MONYYYY
   (the month in any letter case), MM/DD/YYYY with the month first, and
   YYYYMMDD. MONYYYY gives a month alone, which starts on its first day. */
data result;
  length ASTDTC $10 ASTDT $10 TEXT $1024;
  set cm;
  TEXT=upcase(strip(CMSTDAT));
  ASTDTC=''; ASTDT='';
  len=lengthn(TEXT);
  day=.;
  if len=10 and substr(TEXT, 5, 1)='-' then day=input(TEXT, yymmdd10.);
  else if len=9 then day=input(TEXT, date9.);
  else if len=10 and substr(TEXT, 3, 1)='/' then day=input(TEXT, mmddyy10.);
  else if len=8 then day=input(TEXT, yymmdd8.);
  else if len=7 then do;
    first=input(cats('01', TEXT), date9.);
    if not missing(first) then do;
      ASTDTC=substr(put(first, yymmdd10.), 1, 7);
      ASTDT=put(first, yymmdd10.);
    end;
  end;
  if not missing(day) then do;
    ASTDTC=put(day, yymmdd10.);
    ASTDT=ASTDTC;
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDAT, ASTDTC, ASTDT from result;
quit;
proc export data=final outfile="/app/output/adcm.csv" dbms=csv replace; run;
