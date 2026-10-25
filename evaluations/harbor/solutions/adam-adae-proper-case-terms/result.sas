/* opensas reference solution for the yamaa benchmark adam-adae-proper-case-terms.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024.;
run;

/* Proper case as PROPCASE writes it: the first character, and each one that
   directly follows a delimiter, in upper case, and every other letter in
   lower case. Each term is rebuilt one character at a time, a blank
   included. AETERMPC takes the default delimiters, the tab among them, and
   AETERMPB the ones that replace them. The previous character is compared
   with each delimiter: INDEX finds no blank excerpt in this release. */
data result;
  length AETERMPC AETERMPB $1024 CH PREV PC PB $1;
  set ae;
  if missing(AETERM) then do;
    AETERMPC = "Not Reported";
    AETERMPB = "Not Reported";
  end;
  else do;
    do I = 1 to lengthn(AETERM);
      CH = substr(AETERM, I, 1);
      if I = 1 then do;
        AETERMPC = upcase(CH);
        AETERMPB = upcase(CH);
      end;
      else do;
        PREV = substr(AETERM, I - 1, 1);
        if PREV in (" ", "/", "-", "(", ".", '09'x) then PC = upcase(CH);
        else PC = lowcase(CH);
        if PREV in (" ", "/", "-", "(", ".", ",", "[") then PB = upcase(CH);
        else PB = lowcase(CH);
        AETERMPC = substr(AETERMPC, 1, I - 1) || PC;
        AETERMPB = substr(AETERMPB, 1, I - 1) || PB;
      end;
    end;
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, AETERMPC, AETERMPB from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
