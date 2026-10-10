/* opensas reference solution for the yamaa benchmark adam-adsl-new-therapy.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024.;
run;

data cm;
  length STUDYID USUBJID CMCAT CMSCAT CMTRT CMSTDTC CMSTDTC_IMP $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMCAT : $char1024. CMSCAT : $char1024. CMTRT : $char1024. CMSTDTC : $char1024. CMSTDTC_IMP : $char1024.;
run;

data pr;
  length STUDYID USUBJID PRCAT PRSCAT PRTRT PRSTDTC $1024;
  infile "/app/input/pr.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PRSEQ PRCAT : $char1024. PRSCAT : $char1024. PRTRT : $char1024. PRSTDTC : $char1024.;
run;

data medication;
  length therapy $10;
  set cm;
  if CMCAT='ON TREATMENT';
  /* Use the supplied completed date, including its collected corrections. */
  therapy='';
  if lengthn(CMSTDTC_IMP)>=10 then therapy=substr(CMSTDTC_IMP,1,10);
run;
data procedure;
  length therapy $10;
  set pr;
  if PRCAT='CANCER RELATED' and PRSCAT='ON TREATMENT';
  therapy=substr(PRSTDTC,1,10);
run;
data therapies; set medication procedure; keep STUDYID USUBJID therapy; run;
proc sql;
  create table firsttherapy as select a.STUDYID,a.USUBJID,min(b.therapy) as NACTDT from adsl as a inner join therapies as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.TRTSDT ne '' and b.therapy ne '' and b.therapy>=a.TRTSDT group by a.STUDYID,a.USUBJID;
  create table joined as select a.*,b.NACTDT from adsl as a left join firsttherapy as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;
data result;
  length NACTFL $1; set joined;
  NACTFL=''; NACTDY=.;
  if not missing(NACTDT) then do; NACTFL='Y'; NACTDY=input(NACTDT,yymmdd10.)-input(TRTSDT,yymmdd10.)+1; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, TRTSDT, NACTDT, NACTDY, NACTFL from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
