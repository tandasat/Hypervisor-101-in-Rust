# Hypervisor 101 in Rust
The materials of the "Hypervisor 101 in Rust" class held at [Global Cybersecurity Camp 2023 Singapore](https://gcc.ac/gcc_2023/). This repository contains a fuzzing hypervisor for UEFI on Intel/AMD processors, lecture and hands-on exercise materials, and sample corpus and target files.

Read the course at https://tandasat.github.io/Hypervisor-101-in-Rust/


## Directory structure
- 📖[course/](course/) for the class materials
- 🦀[hypervisor/](hypervisor/) for source code and a detailed description of the fuzzing hypervisor
- ⚙️[BUILDING.md](BUILDING.md) for building and funning the hypervisor with sample files under [tests/](tests/)


## Course format
The class materials are designed for an interactive classroom setting and less effective for self-learning due to light explanations. We decided to publish this as it would still be useful to some, however. If you are interested in the interactive class with the author, please check out the schedule of the next public class at [System Programming Lab](https://tandasat.github.io/).


## Supported platforms
- Host (class and development) environment
  - 📎Windows + WSL
  - 🍎macOS
  - 🐧Ubuntu
  - Apple Silicon-based macOS, ARM64-based Windows and Ubuntu are also supported. No x64 system required.
- Test (target) environment
  - Hardware
    - Bochs
    - VMware Fusion or Workstation Pro (if a host has Intel or AMD processors)
    - Select bare metal models
  - with UEFI


## Acknowledgements
- Andrew Burkett (@drewkett) and Rich Seymour (@rseymour) for mentoring me about Rust
- Karsten König (@gr4yf0x) and Amir Bazine for encouraging me looking into use of hypervisors for fuzzing
- Brandon Falk (@gamozolabs) for his inspirational work, Falkvisor
