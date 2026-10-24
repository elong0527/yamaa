/* opensas reference solution for the yamaa benchmark sdtm-ex-infusion-duration.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ex_raw;
  length STUDYID USUBJID EXTRT EXSTDTC $1024;
  infile "/app/input/ex_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024. EXSTDTC : $char1024. EXDURMIN;
run;

/* EXDUR: the collected whole minutes as ISO 8601 duration text, a day the
   largest part and every part that is zero left out; zero minutes is PT0S. */
data result;
  set ex_raw;
  length DOMAIN $2 EXDUR $64;
  DOMAIN = 'EX';
  EXDUR = '';
  if not missing(EXDURMIN) then do;
    if EXDURMIN = 0 then EXDUR = 'PT0S';
    else do;
      days = floor(EXDURMIN / 1440);
      hours = floor((EXDURMIN - days * 1440) / 60);
      mins = EXDURMIN - days * 1440 - hours * 60;
      EXDUR = 'P';
      if days > 0 then EXDUR = cats(EXDUR, strip(put(days, 12.)), 'D');
      if hours > 0 or mins > 0 then do;
        EXDUR = cats(EXDUR, 'T');
        if hours > 0 then EXDUR = cats(EXDUR, strip(put(hours, 12.)), 'H');
        if mins > 0 then EXDUR = cats(EXDUR, strip(put(mins, 12.)), 'M');
      end;
    end;
  end;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXSTDTC, EXDUR from result;
quit;
proc export data=final outfile="/app/output/ex.csv" dbms=csv replace; run;
