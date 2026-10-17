/* opensas reference solution for the yamaa benchmark adam-adpp-period-treatment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data pp;
  length STUDYID DOMAIN USUBJID PPTESTCD PPTEST PPCAT PPSTRESU PPSPEC PPRFTDTC $1024;
  infile "/app/input/pp.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. PPSEQ PPTESTCD : $char1024.
        PPTEST : $char1024. PPCAT : $char1024. PPSTRESN PPSTRESU : $char1024. PPSPEC : $char1024.
        PPRFTDTC : $char1024.;
  ORDER=_n_;
run;

data ex;
  length STUDYID DOMAIN USUBJID EXTRT EXDOSU EXDOSFRM EXROUTE EPOCH EXSTDTC EXENDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024.
        EXDOSE EXDOSU : $char1024. EXDOSFRM : $char1024. EXROUTE : $char1024. EPOCH : $char1024.
        EXSTDTC : $char1024. EXENDTC : $char1024.;
run;

/* The subject's dose that starts exactly at the reference date and time;
   TREATMENT 1 is period 1, TREATMENT 2 period 2. */
proc sql;
  create table final as
  select p.STUDYID, p.USUBJID, p.PPCAT as PARCAT1, p.PPTESTCD as PARAMCD, p.PPTEST as PARAM,
         p.PPRFTDTC, input(scan(e.EPOCH, 2, ' '), best.) as APERIOD, e.EXTRT as TRTA,
         e.EXDOSE as DOSEA, e.EXDOSU as DOSEU, p.PPSTRESN as AVAL, p.PPSTRESU as AVALU
  from pp as p left join ex as e
    on p.USUBJID=e.USUBJID and p.PPRFTDTC=e.EXSTDTC
  order by p.ORDER;
quit;
proc export data=final outfile="/app/output/adpp.csv" dbms=csv replace; run;
