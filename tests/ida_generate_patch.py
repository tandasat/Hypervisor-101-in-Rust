"""Generates the patch file for the IDB file"""
import json
import idaapi  # pylint: disable=import-error
import idautils  # pylint: disable=import-error
import idc  # pylint: disable=import-error


def main():
    """Generates the patch file for the IDB file"""
    idaapi.auto_wait()

    # Get the list of all basic blocks
    block_addrs = []
    for function in idautils.Functions():
        for basic_block in idaapi.FlowChart(idaapi.get_func(function)):
            block_addrs.append(basic_block.start_ea)

    # Generate entries to patch all blocks with int3
    patch_entries = []
    patch = b"\xcc"
    for block_addr in block_addrs:
        original = idaapi.get_bytes(block_addr, len(patch))

        patch_entry = {}
        patch_entry["address"] = block_addr
        patch_entry["length"] = len(patch)
        patch_entry["patch"] = int.from_bytes(patch, byteorder="little")
        patch_entry["original"] = int.from_bytes(original, byteorder="little")
        patch_entries.append(patch_entry)

    # Build the JSON object and write it to a file.
    json_data = {}
    json_data["entries"] = patch_entries

    patch_name = idc.get_idb_path() + "_patch.json"
    with open(patch_name, "w", encoding="utf-8") as outfile:
        json.dump(json_data, outfile, indent=2)
    print(f"Done generating {patch_name}")


if __name__ == "__main__":
    main()
