/* opensas reference solution for the yamaa benchmark sdtm-mh-ongoing-conditions.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data mh_form;
  length STUDYID USUBJID MHTERM MHSTYY MHSTMM MHENDTC MHONGO SCRFDTC $1024;
  infile "/app/input/mh_form.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. MHSEQ MHTERM : $char1024. MHSTYY : $char1024. MHSTMM : $char1024. MHENDTC : $char1024. MHONGO : $char1024. SCRFDTC : $char1024.;
run;

data result; set mh_form; length DOMAIN $2 MHSTDTC MHENRTPT MHENTPT $1024; DOMAIN='MH'; MHSTDTC=''; MHENRTPT=''; MHENTPT='';
if not missing(MHSTYY) then do; MHSTDTC=MHSTYY; if not missing(MHSTMM) then MHSTDTC=catx('-',MHSTYY,MHSTMM); end;
if MHONGO='Y' then do; MHENDTC=''; MHENRTPT='ONGOING'; end; else if not missing(MHENDTC) and not missing(SCRFDTC) and MHENDTC<SCRFDTC then MHENRTPT='BEFORE';
if not missing(MHENRTPT) then MHENTPT='SCREENING'; run;
proc sql;
  create table final as select STUDYID, USUBJID, MHSEQ, MHTERM, MHSTDTC, MHENDTC, MHENRTPT, MHENTPT from result;
quit;
proc export data=final outfile="/app/output/mh.csv" dbms=csv replace; run;
