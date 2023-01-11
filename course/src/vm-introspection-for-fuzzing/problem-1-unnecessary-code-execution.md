# Problem 1: Unnecessary code execution
- The guest continues to run even after the target function finishes
- Our snapshot is taken immediately after the call to `egDecodeAny()` as below
  - No reason to run `FreePool()` and the subsequent code
  ```c
  EG_IMAGE* egLoadImage(EFI_FILE* BaseDir, CHAR16 *FileName, BOOLEAN WantAlpha)
  {
    // ...
    egLoadFile(BaseDir, FileName, &FileData, &FileDataLength)
    newImage = egDecodeAny(FileData, FileDataLength, 128, WantAlpha);
    FreePool(FileData);
    return newImage;
  }
  ```
- Can we abort the guest when `egDecodeAny()` returns?
