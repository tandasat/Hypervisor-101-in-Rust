# Relation to hypervisor
- Nested paging applies only when the processor is in the guest-mode
  - After VM exit, hypervisor runs under traditional paging using the current (host) `CR3`
- Nested paging is two-phased
  - First translation (VA -> GPA) is done exclusively based on guest controlled data (`CR3` and paging structures in guest accessible memory)
    - Failure in this phase results in #PF, which is handled exclusively by the guest using guest IDT
  - Second translation (GPA -> PA) is done exclusively based on hypervisor controlled data (EPT pointer/nCR3 and nested paging structures in guest inaccessible memory)
    - Failure in this phase results in VM exit, which is handled exclusively by the hypervisor
  - AMD: 📖15.25.6 Nested versus Guest Page Faults, Fault Ordering