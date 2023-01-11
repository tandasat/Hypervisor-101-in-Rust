# E#4 Enabling nested paging
- Intel:
  - Set bit[1] of the secondary processor-based VM-execution controls.
  - Set the EPT pointer VMCS to point EPT PML4.
  - 📖Table 25-7. Definitions of Secondary Processor-Based VM-Execution Controls
  - 📖25.6.11 Extended-Page-Table Pointer (EPTP)
- AMD:
  - Set bit[0] of offset 0x90 (NP_ENABLE bit).
  - Set the N_CR3 field (offset 0xb8) to point nested PML4.
  - 📖15.25.3 Enabling Nested Paging
- Expected output: should see normalized VM exit due to `missing_translation: true`
  ```log
  TRACE: NestedPageFaultQualification { rip: dd24e73, gpa: ff77000, missing_translation: true, write_access: true }
  ERROR: panicked at 'not yet implemented: E#5-1', hypervisor\src\hypervisor.rs:172:9
  ```
