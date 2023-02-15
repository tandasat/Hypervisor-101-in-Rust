# Switch to the disk with snapshot, patch and corpus files. fs0:, in this example.
fs0:

# Copy the latest rhv.efi from ISO. fs1: in this example. This is needed for testing
# with VMware, where compiled artifacts are deployed to an ISO file, and not a disk.
copy -q fs1:rhv.efi rhv.efi

# Run rhv.efi.
# E#7-3: Use snapshot_patch_end_marker.json
# E#8-3: Use snapshot_patch.json
rhv.efi snapshot.img snapshot_patch_end_marker.json corpus
