# Read a Spreadsheet Export That Starts with a Byte-Order Mark

**Goal:** carry the site name (`SITENM`) onto a subject listing from
collected demographics saved by a spreadsheet program.

**Input:** one `spec.yaml` reading `input/dm.csv`, saved as UTF-8
comma-separated text the way spreadsheet programs on Windows save it: the
file starts with a byte-order mark, the invisible character U+FEFF that
announces UTF-8 text, and every line ends in a carriage return and a line
feed.

**Variables:**

- `SITENM` is the name of the site the subject enrolled at, as collected,
  accented letters included.

**Note:** the input states that its file starts with a byte-order mark, so
exactly that one mark is skipped and the header is read from the character
after it: the first field is `STUDYID`, with no invisible character in front
of its name. A carriage return before a line feed ends a line like the line
feed alone. The listing is UTF-8 without a mark, and each of its lines ends
in a line feed.
