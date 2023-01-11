# Catching possible bugs
- Types of indicators of bugs and detection of them:
  - Invalid memory access -> #PF interception and nested page fault
  - Use of a non-canonical form memory address -> #GP interception
  - Valid but bogus code execution -> #UD and #BP interception
  - Dead loop -> Timer expiration
- Exploration of those ideas are left for readers
  - The author has not discovered non-dead-loop bugs
