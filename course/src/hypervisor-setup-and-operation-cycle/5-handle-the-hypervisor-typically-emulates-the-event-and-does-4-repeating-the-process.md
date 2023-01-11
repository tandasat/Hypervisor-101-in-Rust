# (5) Handle: The hypervisor typically emulates the event and does (4), repeating the process
- Hypervisor determines the cause of VM exit from the context structure
  - Intel: Exit reason field 📖28.2 RECORDING VM-EXIT INFORMATION AND UPDATING VM-ENTRY CONTROL FIELDS
  - AMD: EXITCODE field 📖15.6 #VMEXIT
- Hypervisor emulates the event on behalf of the guest
  - eg, for `CPUID`
    1. inspects guest's `EAX` and `ECX` as input
    2. updates guest's `EAX`, `EBX`, `ECX`, `EDX` as output
    3. updates guest's `RIP`
- Hypervisor switches to guest with `VMRESUME`/`VMRUN`
