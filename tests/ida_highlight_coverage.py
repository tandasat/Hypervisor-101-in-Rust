"""Highlights executed code blocks based on rhv coverage information"""
import re
import idaapi  # pylint: disable=import-error
import ida_kernwin  # pylint: disable=import-error
import idc  # pylint: disable=import-error


def highlight_basic_block(address: int):
    """Colors a block in the flowchart."""

    color = 0x0000DD
    node_info = idaapi.node_info_t()
    node_info.bg_color = color

    # Find a function and get a graph containing the address.
    function = idaapi.get_func(address)
    for basic_block in idaapi.FlowChart(function):
        if basic_block.start_ea <= address < basic_block.end_ea:
            # Found the corresponding basic block. Add the color to the graph.
            idaapi.set_node_info(
                function.start_ea,
                basic_block.id,
                node_info,
                idaapi.NIF_BG_COLOR,
            )
            # Add the color to each instruction in the basic block.
            for addr in range(basic_block.start_ea, basic_block.end_ea):
                idc.set_color(addr, idc.CIC_ITEM, color)
            break


def main():
    """Highlights executed code blocks based on rhv coverage information"""

    filename = ida_kernwin.ask_file(
        False, "*.txt;*.log", "Open a rhv serial log file for coverage information"
    )
    with open(filename, encoding="UTF-8") as file:
        lines = [line.rstrip() for line in file]

    count = 0
    initial_block = 0
    for line in lines:
        match = re.search(r"COVERAGE: \[(.+)\]", line)
        if not match:
            continue
        # Found a line containing coverage information (ie, a list of basic block
        # addresses). Highlight those basic blocks on IDA.
        addresses = [int(addr, 16) for addr in match.group(1).split(", ")]
        for address in addresses:
            highlight_basic_block(address)
            if initial_block == 0:
                initial_block = address
        count += len(addresses)
    print(f"Highlighted {count} basic blocks.")
    if initial_block:
        print(f"Initial block at 0x{initial_block:x}.")


if __name__ == "__main__":
    main()
