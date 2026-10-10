/* opensas reference solution for the yamaa benchmark adam-adsl-alive-date.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adae;
  length STUDYID USUBJID AENDT $1024;
  infile "/app/input/adae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AENDT : $char1024.;
run;

data adsl;
  length STUDYID USUBJID TRTEDT LSTCNTDC $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTEDT : $char1024. LSTCNTDC : $char1024.;
run;

data advs;
  length STUDYID USUBJID ADATE $1024;
  infile "/app/input/advs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ADATE : $char1024.;
run;

proc sql;
  create table ends as select STUDYID,USUBJID,max(AENDT) as AENDT from adae group by STUDYID,USUBJID;
  create table vitals as select STUDYID,USUBJID,max(ADATE) as ADATE from advs group by STUDYID,USUBJID;
  create table joined as select a.*,b.AENDT,c.ADATE from adsl as a left join ends as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join vitals as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID;
quit;
data result;
  length LSTCNTDT LSTALVDT $10 completed $10;
  set joined;
  if prxmatch('/^[0-9]{4}(-[0-9]{2}(-[0-9]{2})?)?$/',strip(LSTCNTDC)) then do;
    completed=LSTCNTDC;
    if lengthn(LSTCNTDC)=4 then completed=cats(LSTCNTDC,'-01-01');
    if lengthn(LSTCNTDC)=7 then completed=cats(LSTCNTDC,'-01');
    if not missing(input(completed,yymmdd10.)) then LSTCNTDT=completed;
  end;
  lastdate=max(input(TRTEDT,yymmdd10.),input(LSTCNTDT,yymmdd10.),input(AENDT,yymmdd10.),input(ADATE,yymmdd10.));
  if not missing(lastdate) then LSTALVDT=put(lastdate,yymmdd10.);
run;

proc sql;
  create table final as select STUDYID, USUBJID, TRTEDT, LSTCNTDC, LSTCNTDT, LSTALVDT from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
