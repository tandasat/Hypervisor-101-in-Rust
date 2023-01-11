# (2) Set up: The hypervisor creates and sets up a "context structure" representing a guest
- Each guest is represented by a 4KB structure
  - Intel: virtual-machine control structure (VMCS)
  - AMD: virtual machine control block (VMCB)
- It contains fields describing:
  - Guest configurations such as register values
  - Behaviour of HW VT such as what instructions to intercept
  - More details later