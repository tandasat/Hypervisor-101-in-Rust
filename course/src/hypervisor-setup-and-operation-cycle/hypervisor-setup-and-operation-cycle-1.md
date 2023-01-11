# Hypervisor setup and operation cycle
1. Enable: System software enables HW VT and becomes a hypervisor
2. Set up: The hypervisor creates and sets up a "context structure" representing a guest
3. Switch to: The hypervisor asks the processor to load the context structure into hardware-registers and start running in guest-mode
4. Return from: The processor switches back to the host-mode on certain events in the guest-mode
5. Handle: The hypervisor typically emulates the event and does (4), repeating the process.