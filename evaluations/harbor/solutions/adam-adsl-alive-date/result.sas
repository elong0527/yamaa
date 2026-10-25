/* opensas reference solution for the yamaa benchmark adam-adsl-alive-date.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID DTHDTC DTHFL $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DTHDTC : $char1024. DTHFL : $char1024.;
run;

data ex;
  length STUDYID USUBJID EXSEQ EXTRT EXSTDTC EXENDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ : $char1024. EXTRT : $char1024. EXSTDTC : $char1024. EXENDTC : $char1024.;
run;

data ae;
  length STUDYID USUBJID AESEQ AEDECOD AESTDTC AEENDTC AEOUT $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ : $char1024. AEDECOD : $char1024. AESTDTC : $char1024. AEENDTC : $char1024. AEOUT : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSSEQ DSCAT DSDECOD DSTERM DSSTDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ : $char1024. DSCAT : $char1024. DSDECOD : $char1024. DSTERM : $char1024. DSSTDTC : $char1024.;
run;

data sv;
  length STUDYID USUBJID VISITNUM VISIT SVSTDTC $1024;
  infile "/app/input/sv.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VISITNUM : $char1024. VISIT : $char1024. SVSTDTC : $char1024.;
run;

data vs;
  length STUDYID USUBJID VSSEQ VSTESTCD VSDTC $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ : $char1024. VSTESTCD : $char1024. VSDTC : $char1024.;
run;

data ss;
  length STUDYID USUBJID SSSEQ SSTESTCD SSTEST SSSTRESC SSDTC $1024;
  infile "/app/input/ss.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. SSSEQ : $char1024. SSTESTCD : $char1024. SSTEST : $char1024. SSSTRESC : $char1024. SSDTC : $char1024.;
run;

/* Every collected date that shows the subject alive. */
proc sql;
  create table alive_text as
  select STUDYID,USUBJID,EXSTDTC as DTC from ex
  union all select STUDYID,USUBJID,EXENDTC from ex
  union all select STUDYID,USUBJID,AESTDTC from ae
  union all select STUDYID,USUBJID,AEENDTC from ae
  union all select STUDYID,USUBJID,DSSTDTC from ds where DSDECOD not in ('DEATH','LOST TO FOLLOW-UP')
  union all select STUDYID,USUBJID,SVSTDTC from sv
  union all select STUDYID,USUBJID,VSDTC from vs
  union all select STUDYID,USUBJID,SSDTC from ss where SSSTRESC='ALIVE';
quit;

/* A date counts on the day it names: a time is dropped, a year and month
   take the first of the month, and a year alone takes 1 January. */
data alive_days;
  length day $10;
  set alive_text;
  txt=strip(DTC);
  if prxmatch('/^[0-9]{4}-[0-9]{2}-[0-9]{2}/',txt) then day=substr(txt,1,10);
  else if prxmatch('/^[0-9]{4}-[0-9]{2}$/',txt) then day=cats(txt,'-01');
  else if prxmatch('/^[0-9]{4}$/',txt) then day=cats(txt,'-01-01');
  alive=input(day,yymmdd10.);
  if not missing(alive);
  keep STUDYID USUBJID alive;
run;

proc sql;
  create table latest as select STUDYID,USUBJID,max(alive) as alive from alive_days group by STUDYID,USUBJID;
  create table joined as select a.*,b.alive from dm as a left join latest as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

data result;
  length LSTALVDT DTHDT $10 DTHDTF $1;
  set joined;
  t=strip(DTHDTC);
  death=.;
  if prxmatch('/^[0-9]{4}-[0-9]{2}-[0-9]{2}/',t) then death=input(substr(t,1,10),yymmdd10.);
  else if prxmatch('/^[0-9]{4}-[0-9]{2}$/',t) then do;
    death=input(cats(t,'-01'),yymmdd10.);
    DTHDTF='D';
    if not missing(alive) then do;
      if alive>death and substr(put(alive,yymmdd10.),1,7)=t then death=alive;
    end;
  end;
  else if prxmatch('/^[0-9]{4}$/',t) then do;
    death=input(cats(t,'-01-01'),yymmdd10.);
    DTHDTF='M';
    if not missing(alive) then do;
      if alive>death and substr(put(alive,yymmdd10.),1,4)=t then death=alive;
    end;
  end;
  last=alive;
  if not missing(last) and not missing(death) then do;
    if last>death then last=death;
  end;
  if not missing(last) then LSTALVDT=put(last,yymmdd10.);
  if not missing(death) then DTHDT=put(death,yymmdd10.);
  if strip(DTHFL)='Y' or not missing(death) then DTHFL='Y';
  else DTHFL='';
run;

proc sql;
  create table final as select STUDYID, USUBJID, LSTALVDT, DTHFL, DTHDT, DTHDTF from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
