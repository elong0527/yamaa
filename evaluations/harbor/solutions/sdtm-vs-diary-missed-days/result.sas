/* opensas reference solution for the yamaa benchmark sdtm-vs-diary-missed-days.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data vaccination;
  length STUDYID USUBJID VISIT $1024;
  infile "/app/input/vaccination.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DOSENUM VISIT : $char1024. VACDAT : yymmdd10.;
  /* A vaccination with no date was not given and expects no diary. */
  if not missing(VACDAT);
run;

data diary_days;
  length STUDYID DAYLABEL $1024;
  infile "/app/input/diary_days.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DIARYDAY DAYLABEL : $char1024.;
run;

data temperature;
  length STUDYID USUBJID TEMP TEMPU $1024;
  infile "/app/input/temperature.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TEMPDAT : yymmdd10. TEMP : $char1024. TEMPU : $char1024.;
run;

/* Each vaccination given with each diary day; Day 1 is the day of
   vaccination. The entered temperature, when there is one, is matched by
   subject and diary date. */
proc sql;
  create table expected as select v.STUDYID, v.USUBJID, v.VISIT, v.VACDAT, d.DIARYDAY, d.DAYLABEL, v.VACDAT + d.DIARYDAY - 1 as DIARYDAT from vaccination as v inner join diary_days as d on v.STUDYID=d.STUDYID;
  create table joined as select e.*, t.TEMP, t.TEMPU from expected as e left join temperature as t on e.USUBJID=t.USUBJID and e.DIARYDAT=t.TEMPDAT;
quit;

proc sort data=joined; by STUDYID USUBJID VACDAT DIARYDAY; run;
data result;
  length DOMAIN $2 VSTESTCD VSTEST VSORRES VSORRESU VSSTAT VSREASND VSDTC VSTPT VSTPTREF VSRFTDTC $1024;
  set joined;
  by STUDYID USUBJID;
  retain VSSEQ;
  if first.USUBJID then VSSEQ=0;
  VSSEQ+1;
  DOMAIN='VS'; VSTESTCD='TEMP'; VSTEST='Temperature';
  VSORRES=TEMP; VSORRESU=TEMPU; VSSTAT=''; VSREASND='';
  if missing(TEMP) then do; VSSTAT='NOT DONE'; VSREASND='NOT ENTERED'; end;
  VSDTC=put(DIARYDAT, yymmdd10.); VSTPT=DAYLABEL; VSTPTNUM=DIARYDAY;
  VSTPTREF=VISIT; VSRFTDTC=put(VACDAT, yymmdd10.);
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSTEST, VSORRES, VSORRESU, VSSTAT, VSREASND, VSDTC, VSTPT, VSTPTNUM, VSTPTREF, VSRFTDTC from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
