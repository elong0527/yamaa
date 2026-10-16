/* opensas reference solution for the yamaa benchmark adam-adae-post-dose-window.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AEDECOD AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEDECOD : $char1024. AESTDTC : $char1024.;
run;

data ex;
  length STUDYID USUBJID EXTRT EXSTDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024. EXSTDTC : $char1024.;
run;

/* Datetimes collected without seconds are completed to the second so
   onsets and dose starts compare at the moment, to the second. */
data events;
  length ASTDT ASTDTM $19;
  set ae;
  ASTDT=''; ASTDTM='';
  if not missing(AESTDTC) then ASTDT=substr(AESTDTC,1,10);
  if lengthn(AESTDTC)>10 then ASTDTM=cats(AESTDTC,ifc(lengthn(AESTDTC)=16,':00',''));
run;

/* The subject's doses, numbered 1, 2, 3 in order of start. */
proc sort data=ex out=dosesorted; by STUDYID USUBJID EXSTDTC EXSEQ; run;
data doses;
  length DSTART $19;
  set dosesorted;
  by STUDYID USUBJID;
  if first.USUBJID then DOSENUM=0;
  DOSENUM+1;
  DSTART=cats(EXSTDTC,ifc(lengthn(EXSTDTC)=16,':00',''));
  keep STUDYID USUBJID DOSENUM DSTART;
run;

proc sql; create table joined as select a.*,b.DOSENUM,b.DSTART from events as a left join doses as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data window;
  set joined;
  onset=input(ASTDTM,e8601dt.); start=input(DSTART,e8601dt.);
  /* A window opens at the dose start and closes 24 hours later, the
     closing moment itself outside. */
  if not missing(onset) and not missing(start) and onset>=start and onset<start+86400;
run;
proc sort data=window; by STUDYID USUBJID AESEQ start; run;
data picked;
  set window;
  by STUDYID USUBJID AESEQ;
  if last.AESEQ;
  keep STUDYID USUBJID AESEQ DOSENUM;
run;
proc sql;
  create table result as select a.*,b.DOSENUM,case when not missing(b.DOSENUM) then 'Y' else '' end as DOSE24FL
  from events as a left join picked as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, ASTDTM, DOSE24FL, DOSENUM from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
