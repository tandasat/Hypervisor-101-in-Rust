# E#6 Implement copy-on-write and fast memory revert mechanism
- Problem 1: Memory modified by a previous iteration remains modified for a next iteration
  - The hypervisor needs to revert memory, not just registers
- Problem 2: Memory modified by one guest is also visible from other guests, because snapshot-backed memory is shared
  - The hypervisor needs to isolate an effect of memory modification to the current guest
- Solution: Copy-on-write with nested paging
  - (E#6-1) Initially set all pages non-writable through nested paging
  - (E#6-2) On nested page fault due to write-access
    1. change translation to point to non-shared physical address (called "dirty page"),
    2. make it writable,
    3. keep track of those modified nested paging structure entries, and
    4. copy contents of original memory into the dirty page
  - At the end of each fuzzing iteration, revert the all modified nested paging structure entries
- Before triggering copy-on-write
  ```
  [CPU#0] -> [NPSs#0] --\
                         +- Read-only ---------> [Shared memory backed by snapshot]
  [CPU#1] -> [NPSs#1] --/
  ```
- After triggering copy-on-write
  ```
  [CPU#0] -> [NPSs#0] ----- Readable/Writable --> [Private memory]
                         +- Read-only ----------> [Shared memory backed by snapshot]
  [CPU#1] -> [NPSs#1] --/
  ```
- Expected result: No more `Hang detected`, and `Dirty Page#` starts showing numbers🔥
  ```log
  INFO: HH:MM:SS,     Run#, Dirty Page#, New BB#, Total TSC, Guest TSC, VM-exit#,
  INFO: 09:31:09,        1,         131,       0, 916693934,   7982832,      455,
  ...
  INFO: 09:32:22,      297,          30,       0,    803898,    450754,       41,
  INFO: 09:32:25,      298,         131,       0,   8902656,   7919091,      329,
  ```

