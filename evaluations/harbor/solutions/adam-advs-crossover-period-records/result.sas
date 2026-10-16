/* opensas reference solution for the yamaa benchmark adam-advs-crossover-period-records.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRT01A TRT02A AP01SDT AP01SDTM AP02SDT AP02SDTM $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRT01A : $char1024. TRT02A : $char1024. AP01SDT : $char1024. AP01SDTM : $char1024. AP02SDT : $char1024. AP02SDTM : $char1024.;
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
  create table joined as select a.*,b.TRT01A,b.TRT02A,b.AP01SDT,b.AP01SDTM,b.AP02SDT,b.AP02SDTM from collected as a
    left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

/* The latest period whose start is at or before the record: a record
   with a time is compared with the start datetimes, a date-only
   record with the start dates, and a missing start never matches. */
data result;
  length TRTA $1024;
  set joined;
  if not missing(ADTM) and not missing(AP02SDTM) and ADTM>=AP02SDTM then APERIOD=2;
  else if missing(ADTM) and not missing(ADT) and not missing(AP02SDT) and ADT>=AP02SDT then APERIOD=2;
  else if not missing(ADTM) and not missing(AP01SDTM) and ADTM>=AP01SDTM then APERIOD=1;
  else if missing(ADTM) and not missing(ADT) and not missing(AP01SDT) and ADT>=AP01SDT then APERIOD=1;
  if APERIOD=1 then TRTA=TRT01A;
  else if APERIOD=2 then TRTA=TRT02A;
run;

proc sql;
  create table final as select STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
