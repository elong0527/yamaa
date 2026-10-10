/* SAS language reference solution for the yamaa benchmark sdtm-vs-units.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data vs_raw;
  length STUDYID USUBJID VISIT VSTESTCD VSTEST VSORRES VSORRESU $1024;
  infile "/app/input/vs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VISIT : $char1024. VSTESTCD : $char1024. VSTEST : $char1024. VSORRES : $char1024. VSORRESU : $char1024.;
run;

data result; set vs_raw; length DOMAIN $2 VSSTRESC VSSTRESU VSSTAT $1024; DOMAIN='VS'; VSSTRESN=.; VSSTRESC=''; VSSTRESU=''; VSSTAT='';
if missing(VSORRES) then do; VSSTAT='NOT DONE'; VSORRESU=''; end;
else do; VALUE=input(VSORRES,best32.); VSORRES=strip(put(VALUE,best15.)); VSSTRESN=VALUE;
if VSTESTCD='HEIGHT' then VSSTRESU='cm'; else if VSTESTCD='WEIGHT' then do; VSSTRESU='kg'; if VSORRESU='LB' then VSSTRESN=VALUE*0.45359237; end;
else do; VSSTRESU='C'; if VSORRESU='F' then VSSTRESN=(VALUE-32)*5/9; end;
/* Fifteen significant digits; field width alone counts the decimal point.
   Explicit decimal precision avoids BEST32 exposing binary rounding noise. */
if VSSTRESN=0 then VSSTRESC='0';
else do;
  DECIMALS=max(0,14-floor(log10(abs(VSSTRESN))));
  VSSTRESC=strip(putn(VSSTRESN,cats('32.',put(DECIMALS,best12.))));
  if index(VSSTRESC,'.')>0 then do;
    VSSTRESC=prxchange('s/0+$//',1,VSSTRESC);
    VSSTRESC=prxchange('s/\.$//',1,VSSTRESC);
  end;
end;
end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
