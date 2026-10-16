"""Read-only CI diagnostics for Windows loader/import failures. No app launch."""
import ctypes
import os
from pathlib import Path
import sys
import pefile

kernel = ctypes.WinDLL("kernel32", use_last_error=True)
kernel.LoadLibraryExW.argtypes = [ctypes.c_wchar_p, ctypes.c_void_p, ctypes.c_uint]
kernel.LoadLibraryExW.restype = ctypes.c_void_p
kernel.GetProcAddress.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
kernel.GetProcAddress.restype = ctypes.c_void_p
kernel.SetErrorMode(0x0001 | 0x0002 | 0x8000)
path = Path(sys.argv[1])
print("Inspecting", path.name)
pe = pefile.PE(str(path))
missing = 0
for entry in getattr(pe, "DIRECTORY_ENTRY_IMPORT", []):
    dll = entry.dll.decode("ascii")
    handle = kernel.LoadLibraryExW(dll, None, 0x00000800)
    if not handle:
        print("DLL load failed:", dll, "Win32", ctypes.get_last_error())
        missing += 1
        continue
    for item in entry.imports:
        if item.name and not kernel.GetProcAddress(handle, item.name):
            print("Missing entry point:", dll, item.name.decode("ascii"))
            missing += 1
print("Unresolved imports:", missing)
sys.exit(bool(missing))
