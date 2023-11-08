# How to update the book

This instruction is for the repository owner.

## How to update the book

1. Update the `gcc2023` branch (see below).
2. Update the commit hash in `course\src\introduction\README.md` with that of the "Changes for gcc2023" commit. For example with below instructions, it should be `b17a59dd634a7b0c2b9a6d493fc9b0ff22dcfce5`.
3. Build the book (see [README.md](./README.md)).
4. Commit and push the changes
5. Copy the whole contents of `Hypervisor-101-in-Rust\course\book` into `Hypervisor-101-in-Rust` of the `tandasat.github.io` repository.
6. Commit and push this change.


## How to update the `gcc2023` branch

The basic idea is to rebase the branch onto `main` and cherry-pick all 9 additional commits. The following illustrates this operation.

```shell
> git checkout gcc2023
> git log --pretty=oneline
0592b24087201464a94e92d895b4ecbb88caece9 (HEAD -> gcc2023, origin/gcc2023) Solution for E#8
...
e4c948ce1ed2afcdbd63d18f7b0f115d73d91bfc Changes for gcc2023
5bdb5e6d99d638984162b9c55078ea1786e6c5e0 ...

> git reset --hard main
> git cherry-pick 5bdb5e6d99d638984162b9c55078ea1786e6c5e0..0592b24087201464a94e92d895b4ecbb88caece9

> git log --pretty=oneline
58196fb50a2865f0e9358709bd8a11c3838c2f58 (HEAD -> gcc2023) Solution for E#8
...
b17a59dd634a7b0c2b9a6d493fc9b0ff22dcfce5 Changes for gcc2023
c55e339d9d2fda3d62513ab560d2cd1bc758f1b9 (origin/main, origin/HEAD, main) ...

> cargo build
> git push --force-with-lease
```
