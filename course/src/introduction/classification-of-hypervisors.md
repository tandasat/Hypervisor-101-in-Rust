# Classification of hypervisors
- Software-based: Paravirtualization (Xen), binary-translation and emulation (old VMware)
- Hardware-assisted: Uses hardware assisted virtualization technology, HW VT, eg, VT-x and AMD-V (almost any hypervisors)
- vs. emulators/simulators
  - Emulators emulate everything
  - Hypervisors run code on real processors (direct execution) plus minimal use of emulation techniques
  - Many emulators now use HW VT to boost performance: QEMU + KVM, Android Emulator + [HAXM](https://github.com/intel/haxm), [Simics + VMP](https://www.intel.com/content/www/us/en/developer/articles/technical/software-on-wind-river-simics-virtual-platforms-then-and-now.html)
- "Hypervisor" is most often interchangeable with "VMM"
- This class will explicitly talk about Hardware-assisted hypervisors