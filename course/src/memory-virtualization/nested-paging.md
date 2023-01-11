# Nested paging
- VA -> GPA -> PA translation
- When a processor needs to access a given VA, the processor does the following to translate it to a PA, __if__ nested paging is enabled and the processor is in the guest-mode:
1. Performs all of the 10 steps in the previous page, using a guest `CR3` register
2. Treats the resulted value as GPA, instead of PA
3. Locates the top level nested paging structure, nested PML4, from the value in an EPT pointer (Intel) or nCR3 (AMD)
4. Indexes the nested PML4 using `GPA[47:39]` as an index
5. Locates the next level nested paging structure, nested PDPT, from the indexed nested PML4 entry
6. Indexes the nested PDPT using `GPA[38:30]` as an index
7. Locates the next level nested paging structure, nested PD, from the indexed nested PDPT entry
8. Indexes the nested PD using `GPA[29:21]` as an index
9. Locates the next level nested paging structure, nested PT, from the indexed nested PD entry
10. Indexes the nested PT using `GPA[20:12]` as an index
11. Finds out a page frame to translate to, from the indexed nested PT entry
12. Combines the page frame and `GPA[11:0]`, resulting in a PA
- In pseudo code, it would look like this:
  ```python
  # Translate VA to PA with nested paging
  def translate_va_during_guest_mode(va):
      gpa = translate_va(va)    # may raise #PF
      return translate_gpa(gpa) # may cause VM exit

  # Translate VA to (G)PA
  def translate_va(va):
      # Omitted. See the previous page

  # Translate GPA to PA
  def translate_gpa(gpa):
      i4, i3, i2, i1, page_offset = get_indexes(gpa)
      nested_pml4 = read_vmcs(EPT_POINTER) if intel else VMCB.ControlArea.nCR3
      nested_pdpt = nested_pml4[i4]
      nested_pd = nested_pdpt[i3]
      nested_pt = nested_pd[i2]
      page_frame = nested_pt[i1]
      return page_frame | page_offset

  # Get indexes and a page offset from the given address
  def get_indexes(address):
      # Omitted. See the previous page
  ```
- Intel: 📖29.3.2 EPT Translation Mechanism
- AMD: 📖15.25.5 Nested Table Walk