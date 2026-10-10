/* SAS language reference solution for the yamaa benchmark adam-adae-partial-dates.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024.;
run;

data ae;
  length STUDYID USUBJID AETERM AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESTDTC : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDT from ae as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result;
  length ASTDT ASTDTC $10 ASTDTF TRTEMFL $1;
  set joined;
  treatment=input(TRTSDT,yymmdd10.); analysis=.;
  if prxmatch('/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/',strip(AESTDTC)) then analysis=input(AESTDTC,yymmdd10.);
  else if prxmatch('/^[0-9]{4}-[0-9]{2}$/',strip(AESTDTC)) then do;
    analysis=input(cats(AESTDTC,'-15'),yymmdd10.);
    if not missing(analysis) then do;
      if not missing(treatment) and intnx('month',analysis,0,'end')<treatment then analysis=.;
      else do; analysis=max(analysis,treatment); ASTDTF='D'; end;
    end;
  end;
  if not missing(analysis) then do;
    ASTDT=put(analysis,yymmdd10.); ASTDTC=ASTDT;
    if not missing(treatment) and analysis>=treatment then TRTEMFL='Y';
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDT, ASTDTC, ASTDTF, TRTSDT, TRTEMFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
