# Deeper look into guest-mode transition
- Switching to the guest-mode
  - Intel
    - `VMLAUNCH` - used for first transition with the VMCS
    - `VMRESUME` - used for subsequent transitions
  - AMD: `VMRUN`
- Our implementation:

  |     | AMD: `run_vm_svm()`         | Intel: `run_vm_vmx()`                                                            |
  | --- | --------------------------- | -------------------------------------------------------------------------------- |
  | 1   | Save host GPRs into stack   | Save host GPRs into stack                                                        |
  | 2   | Load guest GPRs from memory | Load guest GPRs from memory                                                      |
  | 3   | `VMRUN`                     | if launched { `VMRESUME` } else { set up host `RIP` and `RSP`, then `VMLAUNCH` } |
  | 4   | Save guest GPRs into memory | Save guest GPRs into memory                                                      |
  | 5   | Load host GPRs from stack   | Load host GPRs from stack                                                        |

- Contents of the GPRs are manually switched, because the `VMRUN`, `VMLAUNCH`, `VMRESUME` instructions do not do it