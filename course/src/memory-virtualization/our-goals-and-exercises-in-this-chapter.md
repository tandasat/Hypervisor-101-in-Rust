# Our goals and exercises in this chapter
- E#4: Enable nested paging with empty nested paging structures
- Build up nested paging structures on nested page fault as required, which includes:
  - Handling and normalizing EPT violation and #VMEXIT(NPF)
  - Resolving a PA of a snapshot-backed-page that maps the GPA that caused fault
  - E#5: Building nested paging structures for GPA -> PA translation
- E#6: Implement copy-on-write and fast memory revert mechanism

