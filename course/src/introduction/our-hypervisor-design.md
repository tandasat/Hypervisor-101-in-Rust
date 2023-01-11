# Our hypervisor design
- Creates a VM from the snapshot
- Starts a fuzzing iteration with the VM, meaning:
    1. injecting mutated input into the VM's memory,
    2. letting the VM run, and
    3. observing possible bugs in the VM
- Reverts the VM at the end of each fuzzing iteration
- Runs as many VMs as the number of logical processors on the system
- Is a UEFI program in Rust
- Is tested on Bochs, VMware and select bare metal models