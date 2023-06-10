# (3) Switch to: The hypervisor asks the processor to load the context structure into hardware-registers and start running in guest-mode
- Execution of a special instruction triggers switching to the guest-mode
  - Intel: `VMLAUNCH` or `VMRESUME`
  - AMD: `VMRUN`
- Successful execution of it:
  1. saves current register values into a host-state area
  2. loads register values from the context structure, including `RIP`
  3. changes the processor mode to the guest-mode
  4. starts execution
- A host-state area is:
  - Intel: part of VMCS (host state fields)
  - AMD: separate 4KB block of memory specified by an MSR 📖15.30.4 VM_HSAVE_PA MSR (C001_0117h)
- This host-to-guest-mode transition is called:
  - Intel: VM-entry 📖CHAPTER 27 VM ENTRIES
  - AMD: World switch to guest 📖15.5.1 Basic Operation