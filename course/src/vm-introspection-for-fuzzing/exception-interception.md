# Exception interception
- By default, exception happens within the guest-mode is processed entirely within the guest
  - Delivered through current (guest) IDT
  - Processed by the guest OS
- A hypervisor can optionally intercept them as VM exits
  - Intel: Exception Bitmap VMCS 📖25.6.3 Exception Bitmap
  - AMD: Intercept exception vectors (offset 0x8) 📖15.12 Exception Intercepts
- To enable, set the bits that correspond to exception numbers you want to intercept, eg set `1 << 0xe` to intercept #PF (0xe)

