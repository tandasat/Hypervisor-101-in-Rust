# Another hypervisor design: Deprivileging current execution context
- We start a guest as a completely separate execution context
- Alternatively, a hypervisor can also start a guest based on the current execution context by capturing current register values and setting them into the guest state fields
  - This way, the current system runs on the guest-mode, and a hypervisor intercepts system's operations
  - Type-1 hypervisors do this
  - Common for hypervisors that intend to deeply interact with the OS, eg, as a hypervisor debugger, rootkit, or security enhancement