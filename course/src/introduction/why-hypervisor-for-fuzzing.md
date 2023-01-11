# Why hypervisor for fuzzing
- Advantages:
  - Fuzzing targets are not limited to user-mode
  - Substantially faster than emulators
- Examples
  - Customized hypervisors: [KF/x](https://github.com/intel/kernel-fuzzer-for-xen-project) (Xen), [kAFL/Nyx](https://nyx-fuzz.com/) (KVM), [HyperFuzzer](https://www.microsoft.com/en-us/research/publication/hyperfuzzer-an-efficient-hybrid-fuzzer-for-virtual-cpus/) (Hyper-V)
  - Using hypervisor API: [What The Fuzz](https://github.com/0vercl0k/wtf), [Rewind](https://github.com/quarkslab/rewind), [Hyperpom](https://github.com/Impalabs/hyperpom)
  - Original hypervisors: [FalkVisor](https://github.com/gamozolabs/falkervisor_grilled_cheese), [Barbervisor](https://github.com/Cisco-Talos/Barbervisor)