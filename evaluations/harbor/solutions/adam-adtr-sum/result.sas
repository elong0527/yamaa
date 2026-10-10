/* opensas reference solution for the yamaa benchmark adam-adtr-sum.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data tr;
  length STUDYID USUBJID AVISIT TRLNKID TRGRPID TRTESTCD TRSTAT $1024;
  infile "/app/input/tr.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AVISIT : $char1024. TRSEQ TRLNKID : $char1024. TRGRPID : $char1024. TRTESTCD : $char1024. TRSTRESN TRSTAT : $char1024.;
run;

data trvisit;
  length STUDYID USUBJID AVISIT ADT $1024;
  infile "/app/input/trvisit.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AVISIT : $char1024. AVISITN ADT : $char1024.;
run;

data tu;
  length STUDYID USUBJID TULNKID TUGRPID $1024;
  infile "/app/input/tu.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TULNKID : $char1024. TUGRPID : $char1024.;
run;

proc sql;
  create table inventory as select STUDYID,USUBJID,count(*) as NTARGET from tu where TUGRPID='TARGET' group by STUDYID,USUBJID;
  create table measures as select a.* from tr as a inner join tu as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.TRLNKID=b.TULNKID where a.TRGRPID='TARGET' and a.TRTESTCD='LDIAM' and b.TUGRPID='TARGET';
  create table totals as select STUDYID,USUBJID,AVISIT,sum(TRSTRESN) as AVAL,count(TRSTRESN) as NMEAS from measures group by STUDYID,USUBJID,AVISIT;
  create table combined as select a.*,b.AVAL,b.NMEAS,c.NTARGET from trvisit as a left join totals as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AVISIT=b.AVISIT left join inventory as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID;
quit;
data result;
  length PARAMCD $8 PARAM $40 ANL01FL $1;
  set combined; PARAMCD='SDIAM'; PARAM='Sum of Target Lesion Diameters (mm)';
  if not missing(NMEAS) and not missing(NTARGET) and NMEAS=NTARGET then ANL01FL='Y';
run;

proc sql;
  create table final as select STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS, NTARGET, ANL01FL from result;
quit;
proc export data=final outfile="/app/output/adtr.csv" dbms=csv replace; run;
