/* opensas reference solution for the yamaa benchmark sdtm-vs-study-day.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID RFSTDTC $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RFSTDTC : $char1024.;
run;

data se;
  length STUDYID USUBJID EPOCH SESTDTC SEENDTC $1024;
  infile "/app/input/se.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EPOCH : $char1024. SESTDTC : $char1024. SEENDTC : $char1024.;
run;

data tv;
  length VISIT $1024;
  infile "/app/input/tv.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input VISIT : $char1024. VISITNUM;
run;

data vs_raw;
  length STUDYID USUBJID VISIT VSTESTCD VSORRES VSDTC $1024;
  infile "/app/input/vs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VISIT : $char1024. VSTESTCD : $char1024. VSORRES : $char1024. VSDTC : $char1024.;
run;

proc sql; create table collected as select a.*,b.VISITNUM,c.RFSTDTC from vs_raw a left join tv b on a.VISIT=b.VISIT left join dm c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID; quit;
data dated; set collected; NODATE=missing(VSDTC); run;
proc sort data=dated; by STUDYID USUBJID NODATE VSDTC VSSEQ; run;
data numbered; set dated; by STUDYID USUBJID; retain PRIORNUM; if first.USUBJID then PRIORNUM=.; if not missing(VISITNUM) and not missing(VSDTC) then PRIORNUM=VISITNUM; else if missing(VISITNUM) and not missing(VSDTC) and not missing(PRIORNUM) then VISITNUM=PRIORNUM+0.01; run;
proc sql; create table candidates as select a.*,b.EPOCH,b.SESTDTC from numbered a left join se b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.VSDTC>=b.SESTDTC and a.VSDTC<=b.SEENDTC and not missing(a.VSDTC); quit;
proc sort data=candidates; by STUDYID USUBJID VSSEQ descending SESTDTC; run;
data result; length DOMAIN $2; set candidates; by STUDYID USUBJID VSSEQ; if first.VSSEQ; DOMAIN='VS'; VSDY=.; if lengthn(VSDTC)>=10 and lengthn(RFSTDTC)>=10 then do; VSDY=input(substr(VSDTC,1,10),yymmdd10.)-input(substr(RFSTDTC,1,10),yymmdd10.); if VSDY>=0 then VSDY+1; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, VISIT, VISITNUM, EPOCH, VSDY from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
