/* opensas reference solution for the yamaa benchmark adam-adae-worsening-emergence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDTM $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDTM : $char1024.;
run;

data ae;
  length STUDYID USUBJID AEDECOD AESTDTC AESEV AETOXGR $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEDECOD : $char1024. AESTDTC : $char1024. AESEV : $char1024. AETOXGR : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDTM from ae as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data events;
  length ASTDTM $19;
  set joined; ASTDTM='';
  if not missing(AESTDTC) then ASTDTM=translate(AESTDTC,' ','T');
  onset=input(AESTDTC,e8601dt.); treatment=input(translate(TRTSDTM,'T',' '),e8601dt.);
  grade=input(AETOXGR,best32.);
  select(AESEV); when('MILD') severity=1; when('MODERATE') severity=2; when('SEVERE') severity=3; otherwise severity=.; end;
run;
proc sql;
  create table later as select STUDYID,USUBJID,AEDECOD,max(severity) as maxsev,max(grade) as maxgrade from events where not missing(onset) and not missing(treatment) and onset>=treatment group by STUDYID,USUBJID,AEDECOD;
  create table combined as select a.*,b.maxsev,b.maxgrade from events as a left join later as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AEDECOD=b.AEDECOD;
quit;
data result;
  length TRTEMFL $1; set combined; TRTEMFL='';
  if not missing(onset) and not missing(treatment) then do;
    if onset>=treatment or (not missing(severity) and not missing(maxsev) and maxsev>severity) or (not missing(grade) and not missing(maxgrade) and maxgrade>grade) then TRTEMFL='Y';
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEDECOD, ASTDTM, TRTSDTM, AESEV, AETOXGR, TRTEMFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
