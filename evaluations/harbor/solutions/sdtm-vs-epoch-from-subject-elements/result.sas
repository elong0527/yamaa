/* SAS language reference solution for the yamaa benchmark sdtm-vs-epoch-from-subject-elements.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

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

data collections; set vs_raw; length DTC MOMENT $1024; DTC=VSDTC; MOMENT=''; if lengthn(DTC)=10 then MOMENT=cats(DTC,'T00:00:00'); else if lengthn(DTC)=16 then MOMENT=cats(DTC,':00'); else if lengthn(DTC)>=19 then MOMENT=substr(DTC,1,19); run;
data elements; set se; length DTC MOMENT STARTMOMENT ENDMOMENT $1024; DTC=SESTDTC; MOMENT=''; if lengthn(DTC)=10 then MOMENT=cats(DTC,'T00:00:00'); else if lengthn(DTC)=16 then MOMENT=cats(DTC,':00'); else if lengthn(DTC)>=19 then MOMENT=substr(DTC,1,19); STARTMOMENT=MOMENT; DTC=SEENDTC; MOMENT=''; if lengthn(DTC)=10 then MOMENT=cats(DTC,'T00:00:00'); else if lengthn(DTC)=16 then MOMENT=cats(DTC,':00'); else if lengthn(DTC)>=19 then MOMENT=substr(DTC,1,19); ENDMOMENT=MOMENT; run;
proc sql; create table candidates as select a.*,b.EPOCH,b.STARTMOMENT from collections a left join elements b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.MOMENT>=b.STARTMOMENT and a.MOMENT<=b.ENDMOMENT and not missing(a.MOMENT); quit;
proc sort data=candidates; by STUDYID USUBJID VSSEQ descending STARTMOMENT; run;
data result; length DOMAIN $2; set candidates; DOMAIN='VS'; by STUDYID USUBJID VSSEQ; if first.VSSEQ; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, EPOCH from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
