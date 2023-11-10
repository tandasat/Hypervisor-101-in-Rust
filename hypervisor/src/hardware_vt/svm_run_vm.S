;// The module containing the `run_vm_svm` function.

;// Offsets to each field in the `GuestRegisters` struct.
.set registers_rax, 0x0
.set registers_rbx, 0x8
.set registers_rcx, 0x10
.set registers_rdx, 0x18
.set registers_rdi, 0x20
.set registers_rsi, 0x28
.set registers_rbp, 0x30
.set registers_r8,  0x38
.set registers_r9,  0x40
.set registers_r10, 0x48
.set registers_r11, 0x50
.set registers_r12, 0x58
.set registers_r13, 0x60
.set registers_r14, 0x68
.set registers_r15, 0x70

;// Runs the guest until #VMEXIT occurs.
;//
;// This function works as follows:
;// 1. saves host general purpose register values to stack.
;// 2. loads guest general purpose register values from `GuestRegisters`.
;// 3. executes the VMRUN instruction that
;//     1. saves host register values to the host state area, as specified by
;//        the VM_HSAVE_PA MSR.
;//     2. loads guest register values from the VMCB.
;//     3. starts running code in guest-mode until #VMEXIT.
;// 4. on #VMEXIT, the processor
;//     1. saves guest register values to the VMCB.
;//     2. loads host register values from the host state area.
;//        Some registers are reset to hard-coded values. For example, interrupts
;//        are always disabled.
;//     3. updates VMCB's EXITCODE field with the reason of #VMEXIT.
;//     4. starts running code in host-mode.
;// 5. saves guest general purpose register values to `GuestRegisters`.
;// 6. loads host general purpose register values from stack.
;//
;// Note that state switch implemented here is not complete, and some register
;// values are "leaked" to the other side, for example, XMM registers, and those
;// that are managed with VMSAVE and VMLOAD instructions.
;//
;// See: 15.5 VMRUN Instruction
;//      15.6 #VMEXIT
;//
;// extern "efiapi" fn run_vm_svm(registers: &mut GuestRegisters, guest_vmcb_pa: *const Vmcb);
.global run_vm_svm
run_vm_svm:
    xchg    bx, bx

    ;// Save current (host) general purpose registers onto stack.
    push    rax
    push    rcx
    push    rdx
    push    rbx
    push    rbp
    push    rsi
    push    rdi
    push    r8
    push    r9
    push    r10
    push    r11
    push    r12
    push    r13
    push    r14
    push    r15

    ;// Copy `registers` and `guest_vmcb_pa` for using them. Also, save
    ;// `registers` at the top of stack so that after #VMEXIT, we can find it.
    mov     r15, rcx    ;// r15 <= `registers`
    mov     rax, rdx    ;// rax <= `guest_vmcb_pa`
    push    rcx         ;// [rsp] <= `registers`

    ;// Restore guest general purpose registers from `registers`.
    mov     rbx, [r15 + registers_rbx]
    mov     rcx, [r15 + registers_rcx]
    mov     rdx, [r15 + registers_rdx]
    mov     rdi, [r15 + registers_rdi]
    mov     rsi, [r15 + registers_rsi]
    mov     rbp, [r15 + registers_rbp]
    mov      r8, [r15 + registers_r8]
    mov      r9, [r15 + registers_r9]
    mov     r10, [r15 + registers_r10]
    mov     r11, [r15 + registers_r11]
    mov     r12, [r15 + registers_r12]
    mov     r13, [r15 + registers_r13]
    mov     r14, [r15 + registers_r14]
    mov     r15, [r15 + registers_r15]

    ;// Run the guest until #VMEXIT occurs.
    vmrun   rax

    ;// #VMEXIT occurred. Save current (guest) general purpose registers.
    xchg    bx, bx
    xchg    r15, [rsp]  ;// r15 <= `registers` / [rsp] <= guest r15
    mov     [r15 + registers_rbx], rbx
    mov     [r15 + registers_rcx], rcx
    mov     [r15 + registers_rdx], rdx
    mov     [r15 + registers_rsi], rsi
    mov     [r15 + registers_rdi], rdi
    mov     [r15 + registers_rbp], rbp
    mov     [r15 + registers_r8],  r8
    mov     [r15 + registers_r9],  r9
    mov     [r15 + registers_r10], r10
    mov     [r15 + registers_r11], r11
    mov     [r15 + registers_r12], r12
    mov     [r15 + registers_r13], r13
    mov     [r15 + registers_r14], r14
    mov     rax, [rsp]  ;// rax <= guest r15
    mov     [r15 + registers_r15], rax

.Exit:
    ;// Adjust the stack pointer.
    pop     rax

    ;// Restore host general purpose registers from stack.
    pop     r15
    pop     r14
    pop     r13
    pop     r12
    pop     r11
    pop     r10
    pop     r9
    pop     r8
    pop     rdi
    pop     rsi
    pop     rbp
    pop     rbx
    pop     rdx
    pop     rcx
    pop     rax

    ;// Enable interrupts. Otherwise, UEFI service call will enter dead loop on
    ;// some UEFI implementations such as that of VMware.
    ;// See: 15.17 Global Interrupt Flag, STGI and CLGI Instructions
    stgi
    ret
