# Exercise preparation: Building and running the hypervisor, and navigating code
- You should choose a platform to work on: Intel or AMD.
  - Intel: Use the `rust: cargo xtask bochs-intel` task
  - AMD: Use the `rust: cargo xtask bochs-amd` task
- We assume you are able to:
  - jump to definitions with F12 on VSCode
  - build the hypervisor
  - run it on Bochs. Output should look like this:
    ```log
    INFO: Starting the hypervisor on CPU#0
    ...
    ERROR: panicked at 'not yet implemented: E#1-1', hypervisor/src/hardware_vt/svm.rs:49:9
    ```
  - If not, follow the instructions in [BUILDING](https://github.com/tandasat/Hypervisor-101-in-Rust/blob/main/BUILDING.md)
- Primary code flow for the exercises in this chapter
  ```rust
  // main.rs
  efi_main() {
      start_hypervisor_on_all_processors() {
          start_hypervisor()
      }
  }

  //hypervisor.rs
  start_hypervisor() {
      vm.vt.enable()                      // E#1: Enable HW VT
      vm.vt.initialize()                  // E#2: Configure behaviour of HW VT
      start_vm() {
          vm.vt.revert_registers()        // E#3: Set up guest state based on a snapshot file
          loop {
              // Runs the guest until VM exit
              exit_reason = vm.vt.run()

              // ... (Handles the VM exit)
          }
      }
  }
  ```