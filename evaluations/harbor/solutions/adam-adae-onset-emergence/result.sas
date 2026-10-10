/* SAS language reference solution for the yamaa benchmark adam-adae-onset-emergence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDTM $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDTM : $char1024.;
run;

data ae;
  length STUDYID USUBJID AETERM AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESTDTC : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDTM from ae as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data events;
  length ASTDTM $19 TRTEMFL $1;
  set joined; ASTDTM=''; TRTEMFL='';
  if not missing(AESTDTC) then ASTDTM=translate(AESTDTC,' ','T');
  onset=input(AESTDTC,e8601dt.); treatment=input(translate(TRTSDTM,'T',' '),e8601dt.);
  if not missing(onset) and not missing(treatment) and onset>=treatment then TRTEMFL='Y';
run;
proc sort data=events(where=(TRTEMFL='Y')) out=eligible; by STUDYID USUBJID onset AESEQ; run;
data firstevent; set eligible; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID AESEQ; run;
proc sql; create table result as select a.*,case when not missing(b.AESEQ) then 'Y' else '' end as AOCCFL from events as a left join firstevent as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ; quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, TRTSDTM, TRTEMFL, AOCCFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
