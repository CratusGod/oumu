param([string]$Name = "Oumu", [int]$Top = 12, [int]$ProcId = 0)
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Collections.Generic;
public class MemMap {
  [StructLayout(LayoutKind.Sequential)] public struct MBI { public IntPtr BaseAddress; public IntPtr AllocationBase; public uint AllocationProtect; public ushort PartitionId; public IntPtr RegionSize; public uint State; public uint Protect; public uint Type; }
  [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint a, bool b, int pid);
  [DllImport("kernel32.dll")] static extern IntPtr VirtualQueryEx(IntPtr h, IntPtr addr, out MBI mbi, IntPtr len);
  [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h, IntPtr addr, byte[] buf, IntPtr size, out IntPtr read);
  public static List<string> Scan(int pid, int top) {
    var outp = new List<string>();
    IntPtr h = OpenProcess(0x0410, false, pid);
    long addr = 0; var regions = new List<long[]>();
    long priv = 0, mapped = 0, image = 0;
    var buckets = new SortedDictionary<long, long[]>();
    MBI m;
    while (VirtualQueryEx(h, (IntPtr)addr, out m, (IntPtr)Marshal.SizeOf(typeof(MBI))) != IntPtr.Zero) {
      long size = (long)m.RegionSize;
      if (m.State == 0x1000) {
        if (m.Type == 0x20000) { priv += size; regions.Add(new long[]{ (long)m.BaseAddress, size, m.Protect, (long)m.AllocationBase });
          long b = 1; while (b < size) b <<= 1; if (!buckets.ContainsKey(b)) buckets[b] = new long[2]; buckets[b][0]++; buckets[b][1] += size; }
        else if (m.Type == 0x40000) mapped += size; else image += size;
      }
      addr = (long)m.BaseAddress + size; if (addr <= 0) break;
    }
    outp.Add(String.Format("private {0} MB, mapped {1} MB, image {2} MB", priv >> 20, mapped >> 20, image >> 20));
    foreach (var kv in buckets) if (kv.Value[1] > (8 << 20)) outp.Add(String.Format("  regions <= {0,8} KB: count {1,6}, total {2,5} MB", kv.Key >> 10, kv.Value[0], kv.Value[1] >> 20));
    regions.Sort((a, b) => b[1].CompareTo(a[1]));
    for (int i = 0; i < Math.Min(top, regions.Count); i++) {
      var r = regions[i]; var buf = new byte[4096 * 16]; IntPtr got;
      long zero = 0, total = 0; var sample = new System.Text.StringBuilder();
      for (int k = 0; k < 16; k++) {
        long off = (r[1] / 16) * k; off -= off % 4096;
        if (ReadProcessMemory(h, (IntPtr)(r[0] + off), buf, (IntPtr)4096, out got)) {
          for (int j = 0; j < 4096; j++) { total++; if (buf[j] == 0) zero++; }
          if (k == 1 || k == 8) { for (int j = 0; j < 48; j++) sample.Append(buf[j].ToString("x2")); sample.Append(" | "); }
        }
      }
      outp.Add(String.Format("#{0} base {1:x} size {2} MB prot {3:x} zero {4}% sample {5}", i, r[0], r[1] >> 20, r[2], total > 0 ? zero * 100 / total : -1, sample));
    }
    return outp;
  }
}
"@
if (-not $ProcId) { $ProcId = (Get-Process $Name | Select-Object -First 1).Id }
[MemMap]::Scan($ProcId, $Top)
