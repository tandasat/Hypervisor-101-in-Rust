# E#3: Configuring guest state in the VMCS/VMCB
- Guest state is managed by:
  - Intel: guest state fields in VMCS
  - AMD: state save area in VMCB
- (E#3-1) Configure a guest based on the snapshot
- Some registers are not updated as part of world switch by hardware
  - General purpose registers (GPRs) are examples
  - (E#3-2) Initialize guest GPRs. They need to be manually saved and loaded by software
- Expected output: "physical address not available" error
  - Intel
    ```log
    [CPU0  ]i| | RIP=000000000dd24e73 (000000000dd24e73)
    ...
    (0).[707060553] ??? (physical address not available)
    ...
    ERROR: panicked at '🐛 Non continuable VM exit 0x2', hypervisor\src\hypervisor.rs:126:17
    ```
  - AMD
    ```log
    [CPU0  ]i| | RIP=000000000dd24e73 (000000000dd24e73)
    ...
    [CPU0  ]p| >>PANIC<< exception(): 3rd (14) exception with no resolution
    [CPU0  ]e| WARNING: Any simulation after this point is completely bogus !
    (0).[698607907] ??? (physical address not available)
    ...
    (after pressing the enter key)
    ...
    ERROR: panicked at '🐛 Non continuable VM exit 0x7f', hypervisor\src\hypervisor.rs:126:17
    ```
- 🎉Notice that we __did__ receive VM exit, meaning we __successfully__ switched to guest-mode
