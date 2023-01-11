# E#5 Building nested paging structures and GPA -&gt; PA translation
- `handle_nested_page_fault()` is called on nested page fault with details of the fault
- `resolve_pa_for_gpa()` returns a PA to translate to, for the given GPA
- `build_translation()` should update nested paging structures to translate given GPA to PA
  - Essentially, walking nested paging structures as processors do and updating entries needed for completing translation
- Expected output: It kind of works! Should see repeating fuzzing iterations🤩
  ```log
  TRACE: NestedPageFaultQualification { rip: efe1d20, gpa: efe41b8, missing_translation: true, write_access: false }
  ...
  Console output disabled. Enable the `stdout_stats_report` feature if desired.
  INFO: HH:MM:SS,     Run#, Dirty Page#, New BB#, Total TSC, Guest TSC, VM-exit#,
  INFO: 08:15:34,        1,           0,       0, 1017837505,   7957016,      443,
  ...
  INFO: 08:16:41,        3,           0,       0, 200828781, 200817997,      133,
  DEBUG: Hang detected : "input_3.png" #2 (bit 1 at offset 0 bytes)
  INFO: 08:17:32,        4,           0,       0, 200821817, 200811033,      133,
  DEBUG: Hang detected : "input_3.png" #3 (bit 2 at offset 0 bytes)
  INFO: 08:18:22,        5,           0,       0, 200829433, 200818649,      133,
  DEBUG: Hang detected : "input_3.png" #4 (bit 3 at offset 0 bytes)
  INFO: 08:19:12,        6,           0,       0, 200833797, 200817997,      133,
  DEBUG: Hang detected : "input_3.png" #5 (bit 4 at offset 0 bytes)
  ```
  - Intel: the 2nd iteration may show
    ```log
    ERROR: 🐈 Unhandled VM exit 0xa
    ```
  - But something is not right. Each run causes `Hang detected` and is extremely slow
