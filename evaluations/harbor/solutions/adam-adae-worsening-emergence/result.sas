/* opensas reference solution for the yamaa benchmark adam-adae-worsening-emergence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDTM $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDTM : $char1024.;
run;

data ae;
  length STUDYID USUBJID AEDECOD AESTDTC AEENDTC AESEV AETOXGR $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ
    AEDECOD : $char1024. AESTDTC : $char1024. AEENDTC : $char1024.
    AESEV : $char1024. AETOXGR : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDTM from ae as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data events;
  length ASTDTM AENDTM $19;
  set joined; ASTDTM=''; AENDTM='';
  if not missing(AESTDTC) then ASTDTM=translate(AESTDTC,' ','T');
  if not missing(AEENDTC) then AENDTM=translate(AEENDTC,' ','T');
  onset=input(AESTDTC,e8601dt.); treatment=input(translate(TRTSDTM,'T',' '),e8601dt.);
  ending=input(AEENDTC,e8601dt.);
  missing_onset=missing(onset);
  grade=input(AETOXGR,best32.);
  select(AESEV); when('MILD') severity=1; when('MODERATE') severity=2; when('SEVERE') severity=3; otherwise severity=.; end;
run;
/* Keep unknown onsets last and use AESEQ to break ties. */
proc sort data=events;
  by STUDYID USUBJID AEDECOD missing_onset onset AESEQ;
run;
data chains;
  set events;
  by STUDYID USUBJID AEDECOD;
  retain chain prevend;
  if first.AEDECOD then do; chain=1; prevend=.; end;
  else if missing(prevend) or missing(onset) or prevend ne onset then
    chain=chain+1;
  prevend=ending;
run;
proc sql;
  create table later as
    select STUDYID,USUBJID,AEDECOD,chain,
      max(severity) as maxsev,max(grade) as maxgrade
    from chains
    where not missing(onset) and not missing(treatment) and onset>=treatment
    group by STUDYID,USUBJID,AEDECOD,chain;
  create table combined as
    select a.*,b.maxsev,b.maxgrade from chains as a left join later as b
    on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID
      and a.AEDECOD=b.AEDECOD and a.chain=b.chain;
quit;
data result;
  length TRTEMFL $1; set combined; TRTEMFL='';
  if not missing(onset) and not missing(treatment) then do;
    if onset>=treatment or (not missing(severity) and not missing(maxsev) and maxsev>severity) or (not missing(grade) and not missing(maxgrade) and maxgrade>grade) then TRTEMFL='Y';
  end;
run;

proc sql;
  create table final as
    select STUDYID, USUBJID, AESEQ, AEDECOD, ASTDTM, AENDTM, TRTSDTM,
      AESEV, AETOXGR, TRTEMFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
