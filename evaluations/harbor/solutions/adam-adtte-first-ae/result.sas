/* opensas reference solution for the yamaa benchmark adam-adtte-first-ae.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adae;
  length STUDYID USUBJID ASTDT AEDECOD $1024;
  infile "/app/input/adae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ASTDT : $char1024. AESEQ AEDECOD : $char1024.;
run;

data adsl;
  length STUDYID USUBJID TRTSDT EOSDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024. EOSDT : $char1024.;
run;

proc sort data=adae(where=(ASTDT ne '')) out=events; by STUDYID USUBJID ASTDT AESEQ; run;
data firstevent; set events; by STUDYID USUBJID; if first.USUBJID; run;
proc sql; create table joined as select a.*,b.ASTDT,b.AESEQ from adsl as a left join firstevent as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result;
  length PARAMCD $8 PARAM $40 STARTDT ADT $10 EVNTDESC $20 SRCDOM $4 SRCVAR $16;
  set joined; PARAMCD='TTAE'; PARAM='Time to First Adverse Event'; STARTDT=TRTSDT;
  if not missing(ASTDT) then do; ADT=ASTDT; CNSR=0; EVNTDESC='AE'; SRCDOM='ADAE'; SRCVAR='ASTDT'; SRCSEQ=AESEQ; end;
  else do; ADT=EOSDT; CNSR=1; EVNTDESC='END OF STUDY'; SRCDOM='ADSL'; SRCVAR='EOSDT'; end;
  if not missing(STARTDT) and not missing(ADT) then do;
    if ADT<STARTDT then ADT=STARTDT;
    AVAL=input(ADT,yymmdd10.)-input(STARTDT,yymmdd10.)+1;
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, SRCDOM, SRCVAR, SRCSEQ from result;
quit;
proc export data=final outfile="/app/output/adtte.csv" dbms=csv replace; run;
