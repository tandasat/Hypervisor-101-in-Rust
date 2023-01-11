# Exception handling
- On VM exit, read-only fields in VMCS/VMCB are updated with details of exception

  |       | Exception Number                 | Error Code (if exists)          |
  | ----- | -------------------------------- | ------------------------------- |
  | Intel | VM-exit interruption information | VM-exit interruption error code |
  | AMD   | EXITCODE                         | EXITINFO1                       |

  - Intel: 📖25.9.2 Information for VM Exits Due to Vectored Events
  - AMD: 📖15.12 Exception Intercepts
- The hypervisor can inject exception to deliver what would have been delivered to the guest
  - Intel: 📖27.6 EVENT INJECTION
  - AMD: 📖15.20 Event Injection
- In our case, we will abort the current fuzzing iteration and revering the guest state
