# Bochs Files
Those files are used to start rhv on Bochs through the `cargo xtask bochs-intel` or `cargo xtask bochs-amd` command. For initial setting up of Bochs, see [BUILDING.md](../../BUILDING.md)


## Configuration Files
The provided Bochs configuration files (.bxrc) are sufficient to run rhv on the OVMF provided UEFI shell environment. This can be useful to debug the types of failures that are difficult to diagnose on VMware or bare metal, for example, failure of the `VMLAUNCH` instruction.

For more information about the configuration file, see the [Bochs User Manual](https://bochs.sourceforge.io/doc/docbook/user/bochsrc.html).


## BIOS Files
Those are copy of BIOS files installed through `apt install ovmf vgabios`. Those copy exist to support macOS and native Windows environment.
