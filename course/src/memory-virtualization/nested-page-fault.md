# Nested page fault
- Translation failure with nested paging structures causes VM exit
  - Intel: EPT violation (and few more 📖29.3.3 EPT-Induced VM Exits)
  - AMD: #VMEXIT(NPF)
- Few read-only fields are updated with the details of a failure

  |       | Fault reasons      | GPA tried to translate | VA tried to translate |
  | ----- | ------------------ | ---------------------- | --------------------- |
  | Intel | Exit qualification | Guest-physical address | Guest linear address  |
  | AMD   | EXITINFO1          | EXITINFO2              | Not Available         |

  - Intel: 📖Table 28-7. Exit Qualification for EPT Violations
  - AMD: 📖15.25.6 Nested versus Guest Page Faults, Fault Ordering
- Typical actions by a hypervisor
  - update nested paging structure(s) and let the guest retry the same operation
    - may require TLB invalidation, like an OS has to when it changes paging structures
  - inject an exception to the guest to prevent access