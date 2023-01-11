# (1) Enable: System software enables HW VT and becomes a hypervisor
- HW VT is implemented as an Instruction Set Architecture (ISA) extension
  - Intel: Virtual Machine Extensions (VMX); branded as VT-x
  - AMD: Secure Virtual Machine (SVM) extension; branded as AMD-V
- Steps to enter the host-mode from the traditional mode:
  1. Enable the feature (Intel: `CR4.VMXE`=1 / AMD: `IA32_EFER.SVME`=1)
  2. (Intel-only) Execute the `VMXON` instruction
- Platform specific mode names

  |       | Host-mode          | Guest-mode             |
  | ----- | ------------------ | ---------------------- |
  | Intel | VMX root operation | VMX non-root operation |
  | AMD   | (Not named)        | Guest-mode             |