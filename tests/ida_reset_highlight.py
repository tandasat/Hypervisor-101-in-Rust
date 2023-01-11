"""Resets color of the all basic blocks"""
import idaapi  # pylint: disable=import-error
import idautils  # pylint: disable=import-error
import idc  # pylint: disable=import-error


def main():
    """Resets color of the all basic blocks"""

    for function in idautils.Functions():
        for basic_block in idaapi.FlowChart(idaapi.get_func(function)):
            idaapi.clr_node_info(
                function,
                basic_block.id,
                idaapi.NIF_BG_COLOR,
            )
            for addr in range(basic_block.start_ea, basic_block.end_ea):
                idc.set_color(addr, idc.CIC_ITEM, idc.DEFCOLOR)
    print(f"Reset color of all basic blocks")


if __name__ == "__main__":
    main()
