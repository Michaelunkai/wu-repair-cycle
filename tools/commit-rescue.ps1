# One-shot commit rescue: create a 32GB supplementary pagefile via NtCreatePagingFile.
# Raises the system commit limit IMMEDIATELY (no reboot). Used to unblock CBS repairs
# and the build toolchain when commit is exhausted.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class PF {
  [StructLayout(LayoutKind.Sequential)]
  public struct UNICODE_STRING { public ushort Length; public ushort MaximumLength; public IntPtr Buffer; }
  [StructLayout(LayoutKind.Sequential)]
  public struct TOKEN_PRIVILEGES { public int Count; public long Luid; public int Attr; }
  [DllImport("kernel32.dll")] public static extern IntPtr GetCurrentProcess();
  [DllImport("advapi32.dll", SetLastError=true)] public static extern bool OpenProcessToken(IntPtr h, uint acc, out IntPtr tok);
  [DllImport("advapi32.dll", SetLastError=true)] public static extern bool LookupPrivilegeValue(string sys, string name, out long luid);
  [DllImport("advapi32.dll", SetLastError=true)] public static extern bool AdjustTokenPrivileges(IntPtr tok, bool dis, ref TOKEN_PRIVILEGES newst, int len, IntPtr prev, IntPtr ret);
  [DllImport("ntdll.dll")] public static extern int NtCreatePagingFile(ref UNICODE_STRING f, ref ulong mn, ref ulong mx, uint p);
  public static bool Enable() {
    IntPtr tok;
    if (!OpenProcessToken(GetCurrentProcess(), 0x28, out tok)) return false;
    long luid;
    if (!LookupPrivilegeValue(null, "SeCreatePagefilePrivilege", out luid)) return false;
    TOKEN_PRIVILEGES tp = new TOKEN_PRIVILEGES();
    tp.Count = 1; tp.Luid = luid; tp.Attr = 2;
    return AdjustTokenPrivileges(tok, false, ref tp, 0, IntPtr.Zero, IntPtr.Zero);
  }
  public static int Create(string ntPath, ulong pages) {
    if (!Enable()) return -1;
    UNICODE_STRING us = new UNICODE_STRING();
    us.Buffer = Marshal.StringToHGlobalUni(ntPath);
    us.Length = (ushort)(ntPath.Length * 2);
    us.MaximumLength = (ushort)(ntPath.Length * 2);
    ulong mn = pages, mx = pages;
    return NtCreatePagingFile(ref us, ref mn, ref mx, 0);
  }
}
'@
$r = [PF]::Create('\??\C:\wufix-pagefile.sys', 8388608UL)
$os = Get-CimInstance Win32_OperatingSystem
"WUFIX-PAGEFILE RESULT: $r  (0 = success)"
"COMMIT FREE NOW: {0:N1} GB of {1:N1} GB" -f ($os.FreeVirtualMemory/1MB), ($os.TotalVirtualMemorySize/1MB)
