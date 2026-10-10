/* opensas reference solution for the yamaa benchmark adam-adsl-crossover-periods.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

data ex;
  length STUDYID USUBJID EPOCH EXTRT EXSTDTC EXENDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EPOCH : $char1024. EXTRT : $char1024. EXSTDTC : $char1024. EXENDTC : $char1024.;
run;

data exposure; set ex; NODATE=missing(EXSTDTC); run;
proc sort data=exposure; by STUDYID USUBJID EPOCH NODATE EXSTDTC EXSEQ; run;
data firstex; set exposure; by STUDYID USUBJID EPOCH; if first.EPOCH; run;
proc sql;
create table starts1 as select STUDYID,USUBJID,min(EXSTDTC) as TR01SDT from ex where EPOCH='TREATMENT 1' and not missing(EXSTDTC) group by STUDYID,USUBJID;
create table ends1 as select STUDYID,USUBJID,max(EXENDTC) as TR01EDT from ex where EPOCH='TREATMENT 1' group by STUDYID,USUBJID;
create table starts2 as select STUDYID,USUBJID,min(EXSTDTC) as TR02SDT from ex where EPOCH='TREATMENT 2' and not missing(EXSTDTC) group by STUDYID,USUBJID;
create table ends2 as select STUDYID,USUBJID,max(EXENDTC) as TR02EDT from ex where EPOCH='TREATMENT 2' group by STUDYID,USUBJID;
create table joined as select a.*,b.TR01SDT,c.TR01EDT,d.TR02SDT,e.TR02EDT,f.EXTRT as TRT01A,g.EXTRT as TRT02A from dm a
left join starts1 b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID
left join ends1 c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID
left join starts2 d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID
left join ends2 e on a.STUDYID=e.STUDYID and a.USUBJID=e.USUBJID
left join firstex f on a.STUDYID=f.STUDYID and a.USUBJID=f.USUBJID and f.EPOCH='TREATMENT 1'
left join firstex g on a.STUDYID=g.STUDYID and a.USUBJID=g.USUBJID and g.EPOCH='TREATMENT 2'; quit;
data result; set joined; WASHDUR=.; if not missing(TR01EDT) and not missing(TR02SDT) then WASHDUR=max(0,input(TR02SDT,yymmdd10.)-input(TR01EDT,yymmdd10.)-1); run;
proc sql;
  create table final as select STUDYID, USUBJID, TR01SDT, TR01EDT, TR02SDT, TR02EDT, TRT01A, TRT02A, WASHDUR from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
