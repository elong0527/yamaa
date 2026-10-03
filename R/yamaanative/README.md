# Optional native installation probe

This is a source template. Stage the shared Rust workspace before building:

```sh
python rust/tools/stage_r_package.py /tmp/yamaa-stage/yamaanative
```

Then run `R CMD build /tmp/yamaa-stage/yamaanative` from a temporary output
directory. See [`rust/README.md`](../../rust/README.md) for prerequisites,
installation tests, and the deliberately limited capability of this package.
