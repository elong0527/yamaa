/* opensas reference solution for the yamaa benchmark adam-adex-infusion-duration.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ex;
  length STUDYID USUBJID EXTRT EXROUTE EXSTDTC EXENDTC EXDUR $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024. EXROUTE : $char1024.
        EXSTDTC : $char1024. EXENDTC : $char1024. EXDUR : $char1024.;
run;

/* A date alone has no time of day, and none is assumed; a time without
   seconds counts as second zero. An ISO 8601 duration of days, hours,
   minutes and seconds wins; anything else counts as not collected. */
data final;
  length PARAMCD $6 PARAM $23 AVALU $3;
  set ex;
  PARAMCD='INFDUR'; PARAM='Infusion Duration (min)'; AVALU='min';
  st=.; en=.;
  if lengthn(EXSTDTC)>10 then st=input(cats(EXSTDTC, ifc(lengthn(EXSTDTC)=16, ':00', '')), e8601dt.);
  if lengthn(EXENDTC)>10 then en=input(cats(EXENDTC, ifc(lengthn(EXENDTC)=16, ':00', '')), e8601dt.);
  CALCDUR=.;
  if not missing(st) and not missing(en) then CALCDUR=(en-st)/60;
  COLL=.;
  rx=prxparse('/^P(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+(?:\.\d+)?)S)?)?$/');
  if not missing(EXDUR) and prxmatch(rx, strip(EXDUR)) and lengthn(EXDUR)>1 and substr(EXDUR, lengthn(EXDUR), 1) ne 'T' then do;
    d=input(prxposn(rx, 1, strip(EXDUR)), ?? best.);
    h=input(prxposn(rx, 2, strip(EXDUR)), ?? best.);
    m=input(prxposn(rx, 3, strip(EXDUR)), ?? best.);
    s=input(prxposn(rx, 4, strip(EXDUR)), ?? best.);
    COLL=sum(0, d*1440, h*60, m, s/60);
  end;
  AVAL=coalesce(COLL, CALCDUR);
  format AVAL CALCDUR best32.;
  keep STUDYID USUBJID EXSEQ EXSTDTC EXENDTC EXDUR PARAMCD PARAM AVAL AVALU CALCDUR;
run;

proc sql;
  create table out as select STUDYID, USUBJID, EXSEQ, EXSTDTC, EXENDTC, EXDUR, PARAMCD, PARAM, AVAL, AVALU, CALCDUR
    from final;
quit;
proc export data=out outfile="/app/output/adex.csv" dbms=csv replace; run;
