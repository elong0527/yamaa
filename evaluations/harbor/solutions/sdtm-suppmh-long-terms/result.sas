/* opensas reference solution for the yamaa benchmark sdtm-suppmh-long-terms.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

/* A quoted term can hold commas; DSD reads it as one field. */
data mh_raw;
  length STUDYID USUBJID MHTERM $1024;
  infile "/app/input/mh_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. MHSEQ MHTERM : $char1024.;
run;

/* The term is cut into pieces of at most 200 characters, each ending after
   the last word that ends within its first 200 characters (a word ends
   where a non-blank is followed by a blank); the blanks at the break belong
   to neither piece. The first piece stays on the MH record, and the second
   and third each give one SUPPMH record. */
data result;
  set mh_raw;
  length RDOMAIN IDVAR IDVARVAL QNAM QLABEL QVAL QORIG QEVAL REST PIECE $1024;
  RDOMAIN = 'MH';
  IDVAR = 'MHSEQ';
  IDVARVAL = strip(put(MHSEQ, best32.));
  QORIG = 'Collected';
  QEVAL = '';
  REST = MHTERM;
  do P = 1 to 3;
    LEN = lengthn(REST);
    PIECE = '';
    if LEN > 0 then do;
      if LEN <= 200 then do;
        PIECE = REST;
        REST = '';
      end;
      else do;
        BRK = 0;
        do K = 1 to 200;
          if substr(REST, K, 1) ne ' ' and substr(REST, K + 1, 1) = ' ' then BRK = K;
        end;
        if BRK = 0 then do;
          PIECE = substr(REST, 1, 200);
          NXT = 201;
        end;
        else do;
          PIECE = substr(REST, 1, BRK);
          NXT = BRK + 1;
          SKIP = 1;
          do K = BRK + 1 to LEN;
            if SKIP = 1 and substr(REST, K, 1) = ' ' then NXT = K + 1;
            else SKIP = 0;
          end;
        end;
        if NXT > LEN then REST = '';
        else REST = substr(REST, NXT);
      end;
    end;
    if P = 2 and LEN > 0 then do;
      QNAM = 'MHTERM1';
      QLABEL = 'Reported Term for the Medical History 1';
      QVAL = PIECE;
      output;
    end;
    if P = 3 and LEN > 0 then do;
      QNAM = 'MHTERM2';
      QLABEL = 'Reported Term for the Medical History 2';
      QVAL = PIECE;
      output;
    end;
  end;
run;

proc sql;
  create table final as
  select STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL
  from result;
quit;
proc export data=final outfile="/app/output/suppmh.csv" dbms=csv replace; run;
