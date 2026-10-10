/* SAS language reference solution for the yamaa benchmark sdtm-ds-sequence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ds_raw;
  length STUDY PATNUM INSTANCE DSDECOD DSDTCOL $1024;
  infile "/app/input/ds_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDY : $char1024. PATNUM : $char1024. INSTANCE : $char1024. DSDECOD : $char1024. DSDTCOL : $char1024.;
run;

data prepared; set ds_raw; length DOMAIN $2 STUDYID USUBJID DSCAT DSDTC $1024; DOMAIN='DS'; D=.; DSDTC=''; STUDYID=STUDY; USUBJID=PATNUM;
if DSDECOD='RANDOMIZED' then DSCAT='PROTOCOL MILESTONE'; else DSCAT='DISPOSITION EVENT';
if lengthn(DSDTCOL)=10 then D=input(DSDTCOL,?? yymmdd10.); else if lengthn(DSDTCOL)=7 then D=input(cats(DSDTCOL,'-15'),?? yymmdd10.); else if lengthn(DSDTCOL)=4 then D=input(cats(DSDTCOL,'-06-15'),?? yymmdd10.);
if not missing(D) then DSDTC=put(D,yymmdd10.); NODATE=missing(D); run;
proc sort data=prepared; by STUDYID USUBJID NODATE D; run;
data result; set prepared; by STUDYID USUBJID; retain DSSEQ;
  if first.USUBJID then DSSEQ=0;
  DSSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, DSSEQ, DSDECOD, DSCAT, DSDTC from result;
quit;
proc export data=final outfile="/app/output/ds.csv" dbms=csv replace; run;
