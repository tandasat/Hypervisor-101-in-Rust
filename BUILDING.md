# Build and Test Instructions
- [Build and Test Instructions](#build-and-test-instructions)
  - [Prerequisite software](#prerequisite-software)
  - [Building](#building)
  - [Testing with Bochs](#testing-with-bochs)
  - [Testing with VMware](#testing-with-vmware)
  - [Testing with bare metal](#testing-with-bare-metal)


## Prerequisite software
- On all platforms:
  - git
  - VSCode (for class exercises)
  - The `rust-analyzer` VSCode extension
- On macOS
  - Homebrew
  - Xcode Command Line Tools
- On Ubuntu
  - build-essential
    ```shell
    sudo apt install build-essential
    ```
- On Windows
  - WSL + build-essential
  - The `Remote Development` VSCode extension
  - Note: all commands below should be executed within WSL unless otherwise stated. This includes VSCode. It must be started _from the WSL shell_, not from the Windows native command prompt. Workflow on native Windows is possible but not documented since WSL is still required.


## Building
1. Install Rust, UEFI target support as well as other dependencies required for Rust.
    ```shell
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source "$HOME/.cargo/env"
    rustup +nightly target add x86_64-unknown-uefi
    ```
2. (macOS-only) Change the default toolchain to `stable-x86_64-apple-darwin`.
    ```shell
    rustup default stable-x86_64-apple-darwin
    ```
3. Clone the repo. It can be any location. This document uses `~` as an example.
    ```shell
    cd ~
    git clone git@github.com:tandasat/Hypervisor-101-in-Rust.git
    ```
4. Build the whole workspace.
    ```shell
    cd Hypervisor-101-in-Rust
    cargo build
    ```

On VSCode, the `cargo build` task is also available.


## Testing with Bochs
1. Clone Bochs. It can be any location. This document uses `~` as an example.
    ```shell
    cd ~
    git clone -b gcc git@github.com:tandasat/Bochs.git
    ```
2. Configure, build and install Bochs.
    - On macOS
        ```shell
        cd Bochs/bochs
        sh .conf.macosx
        make
        sudo make install
        ```
    - On Ubuntu and Windows
        ```shell
        sudo apt install ovmf vgabios gcc g++ make
        cd Bochs/bochs
        sh .conf.linux
        make
        sudo make install
        ```
3. Build and run the hypervisor on Bochs
    - On macOS
        ```shell
        brew install p7zip
        brew install mtools
        cd ~/Hypervisor-101-in-Rust/
        cargo xtask bochs-intel
        # or
        cargo xtask bochs-amd
        ```
    - On Ubuntu and Windows
        ```shell
        sudo apt install p7zip-full mtools
        cd ~/Hypervisor-101-in-Rust/
        cargo xtask bochs-intel
        # or
        cargo xtask bochs-amd
        ```

On VSCode, the `cargo xtask bochs-intel` and `cargo xtask bochs-amd` tasks are also available.


## Testing with VMware
Prerequisite software:
- On macOS
  - VMware Fusion Pro
- On Windows and Ubuntu
  - VMware Workstation Pro

1. Build and run the hypervisor on VMware.
    ```shell
    cd ~/Hypervisor-101-in-Rust/
    cargo xtask vmware
    ```
2. When VMware starts and shows a boot option, select "EFI Internal Shell (Unsupported option)".

On VSCode, the `cargo xtask vmware` task is also available.


## Testing with bare metal
To test on bare metal, have a device with serial output. Copy `rhv.efi`, the snapshot, patch, and corpus files into a FAT32 formatted USB thumb drive. Then, boot the test device, start the UEFI shell, and start the `rhv.efi`.
