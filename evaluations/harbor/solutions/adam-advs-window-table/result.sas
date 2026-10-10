/* opensas reference solution for the yamaa benchmark adam-advs-window-table.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data advs_raw;
  length STUDYID USUBJID PARAMCD VISIT ADT $1024;
  infile "/app/input/advs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. VSSEQ VISIT : $char1024. VISITNUM ADT : $char1024. ADY AVAL;
run;

data awindow;
  length STUDYID AVISIT $1024;
  infile "/app/input/awindow.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. AVISIT : $char1024. AVISITN AWLO AWHI AWTARGET;
run;

proc sql;
  create table candidates as select a.*,b.AVISIT,b.AVISITN,b.AWTARGET,a.ADY-b.AWTARGET as AWTDIFF from advs_raw as a left join awindow as b
    on a.STUDYID=b.STUDYID and not missing(a.ADY) and not missing(b.AWLO) and not missing(b.AWHI) and a.ADY>=b.AWLO and a.ADY<=b.AWHI;
quit;
data ordered; set candidates; distance=abs(AWTDIFF); run;
proc sort data=ordered; by STUDYID USUBJID PARAMCD AVISITN distance VSSEQ; run;
data result; length ANL01FL $1; set ordered; by STUDYID USUBJID PARAMCD AVISITN; if first.AVISITN and not missing(AVISITN) then ANL01FL='Y'; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, ADT, ADY, AVAL, AVISIT, AVISITN, AWTARGET, AWTDIFF, ANL01FL from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
