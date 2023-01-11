# (4) Return from: The processor switches back to the host-mode on certain events in the guest-mode
- Certain events are intercepted by the hypervisor
- On that event, the processor:
  1. saves the current register values into the context structure
  2. loads the previously saved register values from memory
  3. changes the processor mode to the host-mode
  4. starts execution
- This guest-to-host-mode transition is called:
  - Intel: VM-exit 📖CHAPTER 28 VM EXITS
  - AMD: #VMEXIT 📖15.6 #VMEXIT
  - We call it as "VM exit"
- Note that guest uses actual registers and actually runs instructions on a processor.
  - There is no "virtual register" or "virtual processor" in a strict senses.
  - HW VT is a mechanism to perform world switches.
    - Akin to task/process context switching
      - VMCS/VMCB = "task/process" struct
      - `VMLAUNCH`/`VMRESUME`/`VMRUN` = context switch to a task
      - VM-exit/#VMEXIT = preempting the task