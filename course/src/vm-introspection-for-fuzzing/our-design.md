# Our Design
- Prepare a patch file, which contains "where" and "with what byte(s) to replace"
  - In our case, the patch file describes a patch for the return address of `egDecodeAny()` with the `UD` instruction
- When starting the hypervisor, an user specifies the patch file through a command line parameter
- On nested page fault, the hypervisor applies the patch if a page being paged-in is listed in the patch file
- The guest will execute the modified code
- The hypervisor intercepts #UD as VM exit using exception interception (more on later)

