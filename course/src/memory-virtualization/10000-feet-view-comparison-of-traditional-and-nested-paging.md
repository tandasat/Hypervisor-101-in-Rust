# 10000 feet-view comparison of traditional and nested paging

|                                                         | Traditional paging | Intel nested paging   | AMD nested paging |
| ------------------------------------------------------- | ------------------ | --------------------- | ----------------- |
| Translation for                                         | VA -> PA (or GPA)  | GPA -> PA             | GPA -> PA         |
| Typically owned and handled by                          | OS (or guest OS)   | Hypervisor            | Hypervisor        |
| Pointer to the PML4                                     | CR3                | EPT pointer           | nCR3              |
| Translation failure                                     | #PF                | EPT violation VM-exit | #VMEXIT(NPF)      |
| Paging structures compared with traditional ones        | (N/A)              | Similar               | Identical         |
| Bit[11:0] of a paging structure entry is                | Flags              | Flags                 | Flags             |
| Bit[N:12] of a paging structure entry contains          | Page frame         | Page frame            | Page frame        |
| Levels of tables for 4KB translation                    | 4                  | 4                     | 4                 |
