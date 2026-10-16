/* opensas reference solution for the yamaa benchmark adam-adlb-limit-imputation.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD LBSTRESC LBSTRESU $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024. LBSTRESC : $char1024. LBSTRESU : $char1024.;
run;

data result;
  length DTYPE $1024;
  set lb;
  PARAMCD = LBTESTCD;
  AVALC = LBSTRESC;
  AVALU = LBSTRESU;
  /* The numeric boundary: the number after any inequality or "=" prefix. */
  NUMTXT = AVALC;
  if substr(NUMTXT, 1, 2) in ('<=', '>=') then NUMTXT = substr(NUMTXT, 3);
  else if substr(NUMTXT, 1, 1) in ('<', '>', '=') then NUMTXT = substr(NUMTXT, 2);
  if NUMTXT ne '' then LIMITVAL = input(NUMTXT, best32.);
  else LIMITVAL = .;
  /* Only a strict inequality receives the one-unit offset. */
  if missing(AVALC) then AVAL = .;
  else if substr(AVALC, 1, 2) in ('<=', '>=') then AVAL = LIMITVAL;
  else if substr(AVALC, 1, 1) = '<' then AVAL = LIMITVAL - 1;
  else if substr(AVALC, 1, 1) = '>' then AVAL = LIMITVAL + 1;
  else AVAL = LIMITVAL;
  if substr(AVALC, 1, 1) in ('<', '>') then DTYPE = 'IMPUTATION';
  else DTYPE = '';
  drop NUMTXT LIMITVAL;
run;

proc sql;
  create table final as select STUDYID, USUBJID, LBSEQ, PARAMCD, AVALC, AVAL, AVALU, DTYPE from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
