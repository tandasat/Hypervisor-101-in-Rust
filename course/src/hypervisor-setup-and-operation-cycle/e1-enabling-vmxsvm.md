# E#1: Enabling VMX/SVM
- Intel: Enable VMX by `CR4.VMXE`=1. Then, enter VMX root operation with the `VMXON` instruction.
- AMD: Enable SVM by `IA32_EFER.SVME`=1
- Expected result: panic at E#2.
  ```log
  INFO: Starting the hypervisor on CPU#0
  ...
  ERROR: panicked at 'not yet implemented: E#2-1', hypervisor/src/hardware_vt/svm.rs:73:9
  ```