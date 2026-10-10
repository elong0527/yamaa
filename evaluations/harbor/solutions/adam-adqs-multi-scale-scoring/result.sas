/* SAS language reference solution for the yamaa benchmark adam-adqs-multi-scale-scoring.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data qs;
  length STUDYID USUBJID VISIT QSCAT QSTESTCD QSTEST $1024;
  infile "/app/input/qs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VISIT : $char1024. QSSEQ QSCAT : $char1024. QSTESTCD : $char1024. QSTEST : $char1024. QSSTRESN;
run;

proc sql; create table collected as select STUDYID,USUBJID,VISIT,QSTESTCD as PARAMCD,QSTEST as PARAM,QSSTRESN as AVAL from qs; quit;
proc sql;
create table anchor0 as select distinct STUDYID,USUBJID,VISIT from collected where PARAMCD='F101';
create table members0 as select * from collected where PARAMCD in ('F101','F102','F103','F104');
create table stats0 as select a.STUDYID,a.USUBJID,a.VISIT,count(b.AVAL) as NANSWER,mean(b.AVAL) as RAWMEAN from anchor0 a left join members0 b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.VISIT=b.VISIT group by a.STUDYID,a.USUBJID,a.VISIT; quit;
data score0; set stats0; length PARAMCD PARAM $1024; PARAMCD='F1SCORE'; PARAM='Physical Functioning Scale Score'; AVAL=.; if NANSWER>=2 then AVAL=100*(1-(RAWMEAN-1)/3); run;
proc sql;
create table anchor1 as select distinct STUDYID,USUBJID,VISIT from collected where PARAMCD='F201';
create table members1 as select * from collected where PARAMCD in ('F201','F202');
create table stats1 as select a.STUDYID,a.USUBJID,a.VISIT,count(b.AVAL) as NANSWER,mean(b.AVAL) as RAWMEAN from anchor1 a left join members1 b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.VISIT=b.VISIT group by a.STUDYID,a.USUBJID,a.VISIT; quit;
data score1; set stats1; length PARAMCD PARAM $1024; PARAMCD='F2SCORE'; PARAM='Role Functioning Scale Score'; AVAL=.; if NANSWER>=1 then AVAL=100*(1-(RAWMEAN-1)/3); run;
proc sql;
create table anchor2 as select distinct STUDYID,USUBJID,VISIT from collected where PARAMCD='S01';
create table members2 as select * from collected where PARAMCD in ('S01','S02','F104');
create table stats2 as select a.STUDYID,a.USUBJID,a.VISIT,count(b.AVAL) as NANSWER,mean(b.AVAL) as RAWMEAN from anchor2 a left join members2 b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.VISIT=b.VISIT group by a.STUDYID,a.USUBJID,a.VISIT; quit;
data score2; set stats2; length PARAMCD PARAM $1024; PARAMCD='SSCORE'; PARAM='Fatigue and Sleep Symptom Scale Score'; AVAL=.; if NANSWER>=2 then AVAL=100*(RAWMEAN-1)/3; run;
proc sql;
create table anchor3 as select distinct STUDYID,USUBJID,VISIT from collected where PARAMCD='G01';
create table members3 as select * from collected where PARAMCD in ('G01','G02');
create table stats3 as select a.STUDYID,a.USUBJID,a.VISIT,count(b.AVAL) as NANSWER,mean(b.AVAL) as RAWMEAN from anchor3 a left join members3 b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.VISIT=b.VISIT group by a.STUDYID,a.USUBJID,a.VISIT; quit;
data score3; set stats3; length PARAMCD PARAM $1024; PARAMCD='GSCORE'; PARAM='Global Health Scale Score'; AVAL=.; if NANSWER>=1 then AVAL=100*(RAWMEAN-1)/6; run;
data result; length PARAMCD PARAM $1024; set collected score0 score1 score2 score3; run;
proc sql;
  create table final as select STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL from result;
quit;
proc export data=final outfile="/app/output/adqs.csv" dbms=csv replace; run;
