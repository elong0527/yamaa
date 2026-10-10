/* opensas reference solution for the yamaa benchmark adam-adlb-closest-visit.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adlb_raw;
  length STUDYID USUBJID PARAMCD ADT $1024;
  infile "/app/input/adlb_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. LBSEQ ADT : $char1024. ADY AVAL;
run;

data candidates;
  length AVISIT $6 ANL01FL $1;
  set adlb_raw;
  if not missing(ADY) and ADY>=8 and ADY<=22 then do;
    AVISIT='WEEK 2'; AWTARGET=15; ADIST=abs(ADY-15);
  end;
run;
proc sort data=candidates; by STUDYID USUBJID PARAMCD AVISIT ADIST descending ADY LBSEQ; run;
data result; set candidates; by STUDYID USUBJID PARAMCD AVISIT; if first.AVISIT and not missing(AVISIT) then ANL01FL='Y'; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, LBSEQ, ADT, ADY, AVAL, AVISIT, AWTARGET, ADIST, ANL01FL from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
