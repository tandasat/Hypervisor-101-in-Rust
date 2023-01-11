# Testing with Bochs
- [Bochs](https://github.com/bochs-emu/Bochs) is a cross-platform open-source x86_64 PC emulator
  - **Extremely** helpful in an early-phase of hypervisor development
  - Capable of emulating both VMX and SVM, even on ARM-based systems
  - Most importantly, you can debug failure of an instruction
    - for example, error log on the failure of `VMLAUNCH`
      ```log
      [CPU0  ]e| VMFAIL: VMCS host state invalid CR0 0x00000000
      ```
  - Built-in debugger
    - GUI (Windows-only)
      ![](Bochs_debugger_gui.png)
    - CLI
      ```log
      h|help - show list of debugger commands
      h|help command - show short command description
      -*- Debugger control -*-
          help, q|quit|exit, set, instrument, show, trace, trace-reg,
          trace-mem, u|disasm, ldsym, slist, addlyt, remlyt, lyt, source
      -*- Execution control -*-
          c|cont|continue, s|step, p|n|next, modebp, vmexitbp
      -*- Breakpoint management -*-
          vb|vbreak, lb|lbreak, pb|pbreak|b|break, sb, sba, blist,
          bpe, bpd, d|del|delete, watch, unwatch
      -*- CPU and memory contents -*-
          x, xp, setpmem, writemem, loadmem, crc, info, deref,
          r|reg|regs|registers, fp|fpu, mmx, sse, sreg, dreg, creg,
          page, set, ptime, print-stack, bt, print-string, ?|calc
      -*- Working with bochs param tree -*-
          show "param", restore
      ```
- Good idea to start with Bochs, then VMware. Last but not least, bare metal
