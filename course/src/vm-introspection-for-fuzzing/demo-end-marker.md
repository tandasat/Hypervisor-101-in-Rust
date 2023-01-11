# Demo: end marker
- Will show:
  1. The patch file `tests/samples/snapshot_patch_end_marker.json`
  2. The patch location with a disassembler
  3. Updating `tests/startup.nsh` to supply the patch file
  4. Running the hypervisor and comparing speed
    - One fuzzing iteration completes about x3 faster in some cases
     - Before the change
       ```log
       INFO: 12:58:06,        2,          30,       0,  28944596,    421124,       43,
       ...
       INFO: 12:58:35,      302,         131,       0,   8229261,   7333745,      206,
       ```
     - After the change
       ```log
       INFO: 13:13:30,        2,           6,       0,     98006,      1634,        7,
       ...
       INFO: 13:13:40,      302,         128,       0,   8163017,   7315310,      200,
       ```
