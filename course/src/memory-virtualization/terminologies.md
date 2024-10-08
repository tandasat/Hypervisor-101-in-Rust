# Terminologies
- In this material:
- Nested paging - the address translation mechanism used during the guest-mode when...
  - Intel: EPT is enabled
  - AMD: Nested paging (also referred to as Rapid Virtualization Indexing, RVI) is enabled
  - Nested paging is also referred to as second level address translation (SLAT)
- Nested paging structures - the data structures describing translation with nested paging
  - Intel: Extended page table(s)
  - AMD: Nested page tables(s)
- Nested page fault - translation fault at the nested paging level
  - Intel: EPT violation
  - AMD: #VMEXIT(NPF)
- (Nested) paging structure entry - an entry of any of (nested) paging structures.
  - Do not be confused with a "(nested) page table entry" which is "an entry of the (nested) page table" specifically
- Virtual address (VA) - same as "linear address" in the Intel manual
- Physical address (PA) - an effective address to be sent to the memory controller for memory access. Same as "system physical address" in the AMD manual
- Guest physical address (GPA) - an intermediate form of an address used when nested paging is enabled (more on later)
