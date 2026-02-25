# export_core_dll_for_pdms.py
# IDAPython script: export core.dll exports, db1-db5 functions, and key structs to JSON.
# Run in IDA with File -> Script file, or: exec(open(r"path/to/export_core_dll_for_pdms.py").read())
# Output: ida_exports/exports.json, db_functions.json, structs.json (sibling of scripts/ = ida_exports/)

from __future__ import print_function
import os
import json

try:
    import idaapi
    import idc
    import idautils
    import ida_funcs
    import ida_name
    import ida_struct
    import ida_nalt
    import ida_entry
except ImportError:
    print("Not running inside IDA: some exports will be skipped.")
    idaapi = idc = idautils = ida_funcs = ida_name = ida_struct = ida_nalt = ida_entry = None

OUTPUT_DIR = None  # set below to ida_exports/ (parent of scripts/)


def _output_dir():
    global OUTPUT_DIR
    if OUTPUT_DIR is not None:
        return OUTPUT_DIR
    script_path = os.path.realpath(__file__)
    # .../ida_exports/scripts/export_core_dll_for_pdms.py -> .../ida_exports
    OUTPUT_DIR = os.path.join(os.path.dirname(os.path.dirname(script_path)))
    return OUTPUT_DIR


def _ensure_dir():
    d = _output_dir()
    if not os.path.isdir(d):
        os.makedirs(d)
    return d


def collect_exports():
    """Collect export table: name, ordinal, address. Fallback to empty if API unavailable."""
    out = []
    if ida_entry is None or ida_nalt is None:
        return {"entries": [], "note": "IDA export API not available (not in IDA or API missing)."}
    try:
        n = ida_entry.get_entry_qty()
    except Exception:
        n = 0
    for i in range(n):
        try:
            ordinal = ida_entry.get_entry_ordinal(i)
            ea = ida_entry.get_entry(ordinal)
            name = ida_name.get_name(ea) or idc.get_func_name(ea) or ("sub_" + hex(ea).replace("0x", "").upper())
            out.append({"name": name, "ordinal": ordinal, "address": hex(ea)})
        except Exception:
            pass
    # If no entries from get_entry_*, DLL exports might be in names only; add db* functions as fallback
    note = "Entry points from ida_entry.get_entry_* (EXE main or limited)."
    if not out:
        note += " Consider using db_functions.json for db1-db5 entry points."
    return {"entries": out, "note": note}


def collect_db_functions():
    """Collect functions whose name contains db1_, db2_, db3_, db4_, db5_."""
    out = []
    if idautils is None or ida_funcs is None:
        return {"functions": [], "note": "Not in IDA or API missing."}
    prefixes = ("db1_", "db2_", "db3_", "db4_", "db5_")
    for ea in idautils.Functions():
        name = ida_funcs.get_func_name(ea) or idc.get_func_name(ea) or ""
        if not name:
            continue
        if any(name.startswith(p) for p in prefixes):
            out.append({"address": hex(ea), "name": name})
    out.sort(key=lambda x: (x["name"], x["address"]))
    return {"functions": out, "count": len(out)}


def collect_structs():
    """Collect structs relevant to index/session/page/claim/free (keyword match on name)."""
    out = []
    if ida_struct is None:
        return {"structs": [], "note": "Not in IDA or ida_struct missing."}
    BADADDR = getattr(ida_nalt, "BADADDR", 0xFFFFFFFF)
    keywords = ("index", "session", "page", "claim", "free", "refno", "btree", "db2", "db3", "header", "bucket")
    try:
        qty = ida_struct.get_struc_qty()
        for i in range(qty):
            sid = ida_struct.get_struc_id_by_idx(i)
            if sid == ida_nalt.BADADDR if hasattr(ida_nalt, "BADADDR") else sid == 0xFFFFFFFF:
                continue
            name = ida_struct.get_struc_name(sid)
            if name is None:
                continue
            name_lower = name.lower()
            if not any(k in name_lower for k in keywords):
                continue
            struc = ida_struct.get_struc(sid)
            if struc is None:
                continue
            size = ida_struct.get_struc_size(struc)
            members = []
            off = ida_struct.get_struc_first_offset(struc)
            while off != BADADDR and off != 0xFFFFFFFF and (size == 0 or off < size):
                m = ida_struct.get_member(struc, off)
                if m:
                    mname = ida_struct.get_member_name(m) or "(unnamed)"
                    msize = ida_struct.get_member_size(m)
                    members.append({"offset": off, "name": mname, "size": msize})
                off = ida_struct.get_struc_next_offset(struc, off)
            out.append({"name": name, "size": size, "members": members})
    except Exception as e:
        return {"structs": [], "note": "Error: " + str(e)}
    out.sort(key=lambda x: x["name"])
    return {"structs": out, "count": len(out)}


def main():
    base = _ensure_dir()
    all_ok = True

    # exports
    data = collect_exports()
    path = os.path.join(base, "exports.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    print("Wrote:", path)

    # db1-db5 functions
    data = collect_db_functions()
    path = os.path.join(base, "db_functions.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    print("Wrote:", path, "({} functions)".format(data.get("count", 0)))

    # structs
    data = collect_structs()
    path = os.path.join(base, "structs.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    print("Wrote:", path, "({} structs)".format(data.get("count", 0)))

    return 0


if __name__ == "__main__":
    main()
