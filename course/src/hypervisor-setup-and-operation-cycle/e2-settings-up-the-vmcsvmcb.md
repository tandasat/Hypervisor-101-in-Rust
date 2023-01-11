# E#2: Settings up the VMCS/VMCB
- Intel:
  - VMCS is already allocated as `self.vmcs_region`
  - VMCS is read and written only through the `VMREAD`/`VMWRITE` instructions
  - The layout of VMCS is undefined. Instead, `VMREAD`/`VMWRITE` take "encoding" (ie, field ID) to specify which field to access
    - 📖APPENDIX B FIELD ENCODING IN VMCS
  - VMCS needs to be "clear", "active" and "current" to be accessed with `VMREAD`/`VMWRITE`
    - (E#2-1, 2-2) Use `VMCLEAR` and `VMPTRLD` to put a VMCS into this state
  - VMCS contains host state fields.
    - On VM-exit, processor state is updated based on the host state fields
    - (E#2-3) Program them with current register values
- AMD:
  - VMCB is already allocated as `self.vmcb`
  - VMCB is read and written through usual memory access.
  - The layout of VMCB is defined.
    - 📖Appendix B Layout of VMCB
  - VMCB does NOT contain host state fields.
    - Instead, another 4KB memory block, called host state area, is used to save host state on `VMRUN`
    - On #VMEXIT, processor state is updated based on the host state area
    - (E#2-1) Write the address of the area to the `VM_HSAVE_PA` MSR. The host state area is allocated as `self.host_state`.
- Expected result: panic at E#3.
  ```log
  INFO: Entering the fuzzing loop🐇
  ERROR: panicked at 'not yet implemented: E#3-1', hypervisor/src/hardware_vt/svm.rs:176:9
  ```