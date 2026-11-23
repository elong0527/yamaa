/* opensas reference solution for the yamaa benchmark adam-advs-crossover-period-records.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRT01A TRT02A AP01SDT AP01SDTM AP01EDT AP01EDTM AP02SDT AP02SDTM AP02EDT AP02EDTM $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRT01A : $char1024. TRT02A : $char1024. AP01SDT : $char1024. AP01SDTM : $char1024. AP01EDT : $char1024. AP01EDTM : $char1024. AP02SDT : $char1024. AP02SDTM : $char1024. AP02EDT : $char1024. AP02EDTM : $char1024.;
run;

data vs;
  length STUDYID USUBJID VSTESTCD VSDTC $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VSTESTCD : $char1024. VSSTRESN VSDTC : $char1024.;
run;

/* The collection date is the date part of VSDTC. ADTM holds the full
   datetime, with seconds added when the collected time has none, and
   is empty when only a date was collected. ISO text compares in
   calendar and clock order. */
data collected;
  length PARAMCD $16 ADT ADTM $19;
  set vs;
  PARAMCD=VSTESTCD; AVAL=VSSTRESN;
  if not missing(VSDTC) then ADT=substr(VSDTC,1,10);
  if index(VSDTC,'T') then do;
    if length(trim(VSDTC))=16 then ADTM=trim(VSDTC)||":00";
    else ADTM=VSDTC;
  end;
run;

proc sql;
  create table joined as select a.*,b.TRT01A,b.TRT02A,b.AP01SDT,b.AP01SDTM,b.AP01EDT,b.AP01EDTM,b.AP02SDT,b.AP02SDTM,b.AP02EDT,b.AP02EDTM from collected as a
    left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

/* The period whose start and end the record lies within, both included.
   Each limit is compared by time when both the record and the limit have
   one, and by date otherwise; a missing date or limit never matches. The
   later period wins when a record lies within both. A period datetime is
   written with seconds, as ADTM is. */
data result;
  length TRTA $1024;
  set joined;
  array sdt{2} $1024 AP01SDT AP02SDT;
  array sdtm{2} $1024 AP01SDTM AP02SDTM;
  array edt{2} $1024 AP01EDT AP02EDT;
  array edtm{2} $1024 AP01EDTM AP02EDTM;
  do p=1 to 2;
    if not missing(ADT) and not missing(sdt{p}) and not missing(edt{p}) then do;
      if not missing(ADTM) and not missing(sdtm{p}) then after_start=(ADTM>=sdtm{p});
      else after_start=(ADT>=sdt{p});
      if not missing(ADTM) and not missing(edtm{p}) then before_end=(ADTM<=edtm{p});
      else before_end=(ADT<=edt{p});
      if after_start and before_end then APERIOD=p;
    end;
  end;
  if APERIOD=1 then TRTA=TRT01A;
  else if APERIOD=2 then TRTA=TRT02A;
run;

proc sql;
  create table final as select STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
