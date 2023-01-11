# Introduction to patching
- There may not be any VM exit on return, but we can replace code with something that can cause VM exit
  - For example, code can be replaced with the `UD` instruction, which causes #UD, which can be intercepted as VM exit
- The hypervisor can modify memory when paging it in from the snapshot
