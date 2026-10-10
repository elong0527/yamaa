/* SAS language reference solution for the yamaa benchmark adam-advs-carryforward.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024.;
run;

data plan;
  length STUDYID USUBJID PARAMCD ADT $1024;
  infile "/app/input/plan.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ASEQ PARAMCD : $char1024. ADT : $char1024.;
run;

data vs;
  length STUDYID USUBJID VSTESTCD VSDTC $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VSTESTCD : $char1024. VSDTC : $char1024. VSSTRESN;
run;

proc sql;
  create table heights as select a.STUDYID,a.USUBJID,a.VSSTRESN as HEIGHTBL,a.VSDTC,a.VSSEQ from vs as a inner join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID
    where a.VSTESTCD='HEIGHT' and not missing(a.VSSTRESN) and a.VSDTC ne '' and a.VSDTC<=b.TRTSDT;
quit;
proc sort data=heights; by STUDYID USUBJID descending VSDTC descending VSSEQ; run;
data height; set heights; by STUDYID USUBJID; if first.USUBJID; run;
proc sql;
  create table collected as select a.*,b.VSSEQ,b.VSSTRESN as AVALCOL,c.TRTSDT,d.HEIGHTBL from plan as a
    left join vs as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARAMCD=b.VSTESTCD and a.ADT=b.VSDTC
    left join adsl as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID left join height as d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID;
quit;
proc sort data=collected; by STUDYID USUBJID PARAMCD ADT ASEQ; run;
data result;
  retain carried; set collected; by STUDYID USUBJID PARAMCD;
  if first.PARAMCD then carried=.;
  if not missing(AVALCOL) then carried=AVALCOL;
  AVAL=carried;
run;

proc sql;
  create table final as select STUDYID, USUBJID, ASEQ, VSSEQ, PARAMCD, ADT, AVAL, TRTSDT, HEIGHTBL from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
