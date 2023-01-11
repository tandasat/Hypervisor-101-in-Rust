# x64 traditional paging
- VA -> PA translation
- When a processor needs to access a given VA, `va`, the processor does the following to translate it to a PA:
  1. Locates the top level paging structure, PML4, from the value in the `CR3` register
  2. Indexes the PML4 using `va[47:39]` as an index
  3. Locates the next level paging structure, PDPT, from the indexed PML4 entry
  4. Indexes the PDPT using `va[38:30]` as an index
  5. Locates the next level paging structure, PD, from the indexed PDPT entry
  6. Indexes the PD using `va[29:21]` as an index
  7. Locates the next level paging structure, PT, from the indexed PD entry
  8. Indexes the PT using `va[20:12]` as an index
  9. Finds out a page frame to translate to, from the indexed PT entry
  10. Combines the page frame and `va[11:0]`, resulting in a PA
- In pseudo code, it looks like this:
  ```python
  # Translate VA to PA
  def translate_va(va):
      i4, i3, i2, i1, page_offset = get_indexes(va)
      pml4 = cr3()
      pdpt = pml4[i4]
      pd = pdpt[i3]
      pt = pd[i2]
      page_frame = pt[i1]
      return page_frame | page_offset

  # Get indexes and a page offset from the given address
  def get_indexes(address):
      i4 = (address >> 39) & 0b111_111_111
      i3 = (address >> 30) & 0b111_111_111
      i2 = (address >> 21) & 0b111_111_111
      i1 = (address >> 12) & 0b111_111_111
      page_offset = address & 0b111_111_111_111
      return i4, i3, i2, i1, page_offset
  ```
- Intel: 📖Figure 4-8. Linear-Address Translation to a 4-KByte Page using 4-Level Paging
- AMD: 📖Figure 5-17. 4-Kbyte Page Translation-Long Mode 4-Level Paging
