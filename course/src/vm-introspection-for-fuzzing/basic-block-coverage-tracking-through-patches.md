# Basic-block coverage tracking through patches
- Idea
  - Patch the beginning of every basic block of a target and trigger VM exit when a guest executes them
  - When such VM exit occurs, remove the patch (replace a byte with an original byte) so that future execution does not cause VM exit
  - VM exit due to the patch == execution of a new basic block == good input
- Implemented in a variety of fuzzers, eg, [mesos](https://github.com/gamozolabs/mesos), [ImageIO](https://googleprojectzero.blogspot.com/2020/04/fuzzing-imageio.html), [Hyntrospect](https://github.com/googleprojectzero/Hyntrospect), [what the fuzz](https://github.com/0vercl0k/wtf), [KF/x](https://github.com/intel/kernel-fuzzer-for-xen-project)
- Some of other ideas explained in the [Putting the Hype in Hypervisor](https://www.youtube.com/watch?v=4nz-7ktdU_k) talk
  - Intel Processor Trace
  - Branch single stepping
  - Interrupt/timer based sampling
