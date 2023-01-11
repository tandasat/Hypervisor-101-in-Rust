# Memory virtualization
- We saw
  > physical address not available
- Memory is not virtualized for the guest
- When the guest translates VA to PA using the guest `CR3`
  - as-is
    - the translated PA is used to access physical memory
    - the guest could read and write any memory (including hypervisor's or other guests' memory)
    - In our case, a PA the guest attempted to access was not available
  - with memory virtualization:
    - the translated PA is again translated using hypervisor managed mapping
    - the hypervisor can prevent guest from accessing hypervisor's or other guests' memory