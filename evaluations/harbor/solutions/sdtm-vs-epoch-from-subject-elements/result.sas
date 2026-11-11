/* opensas reference solution for the yamaa benchmark sdtm-vs-epoch-from-subject-elements.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data se;
  length STUDYID USUBJID ETCD ELEMENT EPOCH SESTDTC SEENDTC $1024;
  infile "/app/input/se.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ETCD : $char1024. ELEMENT : $char1024. EPOCH : $char1024. SESTDTC : $char1024. SEENDTC : $char1024.;
run;

data vs_raw;
  length STUDYID USUBJID VSTESTCD VSORRES VSDTC $1024;
  infile "/app/input/vs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ VSTESTCD : $char1024. VSORRES : $char1024. VSDTC : $char1024.;
run;

/* A date-time is placed by moment. A date alone is compared by date, with no
   time assumed: an element whose start date is on or before it and whose end
   date is on or after it, which is an element starting by the end of that
   day and ending at or after its start. */
data collections; set vs_raw; length DTC KEYLO KEYHI $1024; DTC=VSDTC; KEYLO=''; KEYHI=''; if lengthn(DTC)=10 then do; KEYLO=cats(DTC,'T23:59:59'); KEYHI=cats(DTC,'T00:00:00'); end; else if lengthn(DTC)=16 then do; KEYLO=cats(DTC,':00'); KEYHI=KEYLO; end; else if lengthn(DTC)>=19 then do; KEYLO=substr(DTC,1,19); KEYHI=KEYLO; end; run;
data elements; set se; length DTC MOMENT STARTMOMENT ENDMOMENT $1024; DTC=SESTDTC; MOMENT=''; if lengthn(DTC)=10 then MOMENT=cats(DTC,'T00:00:00'); else if lengthn(DTC)=16 then MOMENT=cats(DTC,':00'); else if lengthn(DTC)>=19 then MOMENT=substr(DTC,1,19); STARTMOMENT=MOMENT; DTC=SEENDTC; MOMENT=''; if lengthn(DTC)=10 then MOMENT=cats(DTC,'T00:00:00'); else if lengthn(DTC)=16 then MOMENT=cats(DTC,':00'); else if lengthn(DTC)>=19 then MOMENT=substr(DTC,1,19); ENDMOMENT=MOMENT; run;
proc sql; create table candidates as select a.*,b.EPOCH,b.STARTMOMENT from collections a left join elements b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.KEYLO>=b.STARTMOMENT and a.KEYHI<=b.ENDMOMENT and not missing(a.KEYLO); quit;
proc sort data=candidates; by STUDYID USUBJID VSSEQ descending STARTMOMENT; run;
data result; length DOMAIN $2; set candidates; DOMAIN='VS'; by STUDYID USUBJID VSSEQ; if first.VSSEQ; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, EPOCH from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
