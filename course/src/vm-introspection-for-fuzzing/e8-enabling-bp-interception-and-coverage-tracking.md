# E#8 Enabling #BP interception and coverage tracking
- Change `tests/startup.nsh` to use `snapshot_patch.json`
- Expected result: `New BB#` starts showing numbers. `COVERAGE:` appears in the log.
  ```log
  INFO: HH:MM:SS,     Run#, Dirty Page#, New BB#, Total TSC, Guest TSC, VM-exit#,
  ...
  INFO: 14:37:36,        2,           6,       8,   5361191,      3170,       15,
  INFO: COVERAGE: [dd2d8df, dd26544, dd24ea8, dd25cea, dd25cf4, dd25d0f, dd25d08, dd25f78]
  TRACE: Reached the end marker
  DEBUG: Adding a new input file "input_3.png_1". Remaining 3
  ```
