/* SAS language reference solution for the yamaa benchmark bimo-clinsite-site-level.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID SITENUM ARM SAFFL $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. SITENUM : $char1024. ARM : $char1024. SAFFL : $char1024.;
run;

proc sql;
create table counts as select STUDYID,SITENUM,count(*) as ENRLPOP,sum(case when SAFFL='Y' then 1 else 0 end) as SAFPOP from adsl group by STUDYID,SITENUM;
create table arms as select STUDYID,SITENUM,min(ARM) as ARM from adsl where SAFFL='Y' group by STUDYID,SITENUM;
create table result as select a.STUDYID,a.SITENUM as SITEID,b.ARM,a.SAFPOP,a.ENRLPOP from counts a left join arms b on a.STUDYID=b.STUDYID and a.SITENUM=b.SITENUM;
quit;
proc sql;
  create table final as select STUDYID, SITEID, ARM, SAFPOP, ENRLPOP from result;
quit;
proc export data=final outfile="/app/output/clinsite.csv" dbms=csv replace; run;
