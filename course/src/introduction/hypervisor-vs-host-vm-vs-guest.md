# Hypervisor vs host, VM vs guest
- Those are interchangeable
- When HW VT is in use, a processor runs in one of two modes:
  - host-mode: the same as usual + a few HW VT related instructions are usable.
    - Intel: VMX root operation
      > 24.3 INTRODUCTION TO VMX OPERATION
      >
      > (...) Processor behavior in VMX root operation is very much as it is outside VMX operation.
    - AMD: (even does not defined the term)
  - guest-mode: the restricted mode where some operations are intercepted by a hypervisor
    - Intel: VMX non-root operation
      > 24.3 INTRODUCTION TO VMX OPERATION
      >
      > (...) Processor behavior in VMX non-root operation is restricted and modified to facilitate virtualization. Instead of their ordinary operation, certain instructions (...) and events cause VM exits to the VMM.
    - AMD: Guest-mode
      > 15.2.2 Guest Mode
      >
      > This new processor mode is entered through the VMRUN instruction. When in guest-mode, the behavior of some x86 instructions changes to facilitate virtualization.
- The execution context in the host-mode == hypervisor == host
- The execution context in the guest-mode == VM == guest
  - Intel: 📖24.2 VIRTUAL MACHINE ARCHITECTURE
  - AMD: 📖15.1 The Virtual Machine Monitor
- NB: those are based on the specs. In context of particular software/product architecture, those terms can be used with different definitions
- This "mode" is an orthogonal concept to the ring-level
  - For example, ring-0 guest-mode and ring-3 host-mode are possible and normal
