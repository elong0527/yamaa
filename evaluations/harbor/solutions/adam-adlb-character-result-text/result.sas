/* opensas reference solution for the yamaa benchmark adam-adlb-character-result-text.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID PARAMCD $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. AVAL;
run;

/* Two decimals right-aligned in eight characters, or written in full when
   wider; one decimal without leading spaces. ROUND sends a half away from
   zero, and a result of zero is written without a minus sign. */
data result;
  set lb;
  length AVALC AVALCS $32;
  if missing(AVAL) then AVALC = "NOT DONE";
  else do;
    AVALC = strip(put(round(AVAL, 0.01), 32.2));
    if AVALC = "-0.00" then AVALC = "0.00";
    if length(AVALC) < 8 then AVALC = right(put(AVALC, $8.));
    AVALCS = strip(put(round(AVAL, 0.1), 32.1));
    if AVALCS = "-0.0" then AVALCS = "0.0";
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, AVAL, AVALC, AVALCS from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
