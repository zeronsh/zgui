"""Performance counters (PDH) for what a process can't read of another: the
Desktop Window Manager's CPU time (`dwm.exe` opens to no other user's
process) and any process's GPU engine time; and any process's CPU cycles
(`cycles_of`). Raw cumulative values, so a span is the difference of two
samples; no administrator rights needed.

    with Counters([dwm_cpu(), gpu_time(pid)]) as counters:
        first = counters.sample()
        ...
        last = counters.sample()
"""
import ctypes
import subprocess
from ctypes import wintypes

pdh = ctypes.WinDLL("pdh")
for _name in ("PdhOpenQueryW", "PdhAddEnglishCounterW", "PdhCollectQueryData", "PdhGetRawCounterArrayW",
              "PdhGetFormattedCounterValue", "PdhCloseQuery"):
    # PDH_STATUS: unsigned, as its codes are written (PDH_MORE_DATA).
    getattr(pdh, _name).restype = wintypes.DWORD
PDH_MORE_DATA = 0x800007D2


class PDH_RAW_COUNTER(ctypes.Structure):
    _fields_ = [("CStatus", wintypes.DWORD), ("TimeStamp", wintypes.FILETIME),
                ("FirstValue", ctypes.c_longlong), ("SecondValue", ctypes.c_longlong),
                ("MultiCount", wintypes.DWORD)]


class PDH_RAW_COUNTER_ITEM_W(ctypes.Structure):
    _fields_ = [("szName", wintypes.LPWSTR), ("RawValue", PDH_RAW_COUNTER)]


def pid_of(image):
    """The process id of the one running `image` (`dwm.exe`)."""
    out = subprocess.run(["tasklist", "/fi", f"imagename eq {image}", "/fo", "csv", "/nh"],
                         capture_output=True, text=True).stdout
    return int(out.split(",")[1].strip('"'))


def dwm_cpu():
    """The compositor's CPU time, in 100 ns units."""
    return r"\Process(dwm)\% Processor Time"


ntdll = ctypes.WinDLL("ntdll")
_processes = ctypes.create_string_buffer(1 << 20)


def cycles_of(pid):
    """`pid`'s CPU cycles so far, any process's: from the system's process
    list (`SystemProcessInformation`), as `QueryProcessCycleTime` needs a
    handle `dwm.exe` gives no other user. None if it is not running."""
    global _processes
    while True:
        needed = wintypes.ULONG()
        status = ntdll.NtQuerySystemInformation(5, _processes, len(_processes), ctypes.byref(needed)) & 0xFFFFFFFF
        if status != 0xC0000004:  # STATUS_INFO_LENGTH_MISMATCH: a bigger buffer
            break
        _processes = ctypes.create_string_buffer(needed.value + (1 << 16))
    if status:
        raise OSError(f"NtQuerySystemInformation: {status:#x}")
    # SYSTEM_PROCESS_INFORMATION (64-bit): NextEntryOffset at 0, CycleTime
    # at 24, UniqueProcessId at 80.
    offset = 0
    while True:
        if int.from_bytes(_processes[offset + 80:offset + 88], "little") == pid:
            return int.from_bytes(_processes[offset + 24:offset + 32], "little")
        step = int.from_bytes(_processes[offset:offset + 4], "little")
        if not step:
            return None
        offset += step


def gpu_time(pid, engine="3D"):
    """`pid`'s running time on the GPU's `engine` engines (3D: where both
    an app's frames and the compositor's are drawn), in 100 ns units."""
    return rf"\GPU Engine(pid_{pid}_*_engtype_{engine})\Running Time"


class PDH_FMT_COUNTERVALUE(ctypes.Structure):
    _fields_ = [("CStatus", wintypes.DWORD), ("doubleValue", ctypes.c_double)]


class Rate:
    """One counter read as a rate, over the time since the last sample (the
    processors' performance): `sample()` gives it as PDH formats it."""

    def __init__(self, path):
        self.query, self.counter = wintypes.HANDLE(), wintypes.HANDLE()
        if pdh.PdhOpenQueryW(None, 0, ctypes.byref(self.query)) or pdh.PdhAddEnglishCounterW(
                self.query, path, 0, ctypes.byref(self.counter)):
            raise OSError(f"PDH could not open {path}")
        pdh.PdhCollectQueryData(self.query)

    def sample(self):
        pdh.PdhCollectQueryData(self.query)
        value = PDH_FMT_COUNTERVALUE()
        if pdh.PdhGetFormattedCounterValue(self.counter, 0x200, None, ctypes.byref(value)):  # PDH_FMT_DOUBLE
            return None
        return value.doubleValue

    def close(self):
        pdh.PdhCloseQuery(self.query)


def processor_performance():
    """The processors' clock as a percentage of their nominal frequency
    (a `Rate`): above 100 while boosting, below while Windows clocks down."""
    return r"\Processor Information(_Total)\% Processor Performance"


class Counters:
    """Counter paths sampled together: each sample is a list of the raw
    cumulative values, summed over a wildcard's instances."""

    def __init__(self, paths):
        self.paths = list(paths)
        self.query = wintypes.HANDLE()
        status = pdh.PdhOpenQueryW(None, 0, ctypes.byref(self.query))
        if status:
            raise OSError(f"PdhOpenQuery: {status:#x}")
        self.counters = []
        for path in self.paths:
            counter = wintypes.HANDLE()
            status = pdh.PdhAddEnglishCounterW(self.query, path, 0, ctypes.byref(counter))
            if status:
                raise OSError(f"PdhAddEnglishCounter {path}: {status:#x}")
            self.counters.append(counter)

    def sample(self):
        pdh.PdhCollectQueryData(self.query)
        values = []
        for counter in self.counters:
            size, count = wintypes.DWORD(0), wintypes.DWORD(0)
            status = pdh.PdhGetRawCounterArrayW(counter, ctypes.byref(size), ctypes.byref(count), None)
            if status != PDH_MORE_DATA or not size.value:
                values.append(0)  # no instance (yet): nothing ran
                continue
            buffer = (ctypes.c_byte * size.value)()
            status = pdh.PdhGetRawCounterArrayW(counter, ctypes.byref(size), ctypes.byref(count), buffer)
            if status:
                values.append(0)
                continue
            items = ctypes.cast(buffer, ctypes.POINTER(PDH_RAW_COUNTER_ITEM_W))
            values.append(sum(items[i].RawValue.FirstValue for i in range(count.value)))
        return values

    def close(self):
        pdh.PdhCloseQuery(self.query)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


if __name__ == "__main__":
    import time
    dwm = pid_of("dwm.exe")
    with Counters([dwm_cpu(), gpu_time(dwm)]) as counters:
        first, cycles, start = counters.sample(), cycles_of(dwm), time.perf_counter()
        time.sleep(2)
        last, cycles, wall = counters.sample(), cycles_of(dwm) - cycles, time.perf_counter() - start
    cpu, gpu = ((b - a) / 1e7 for a, b in zip(first, last))
    print(f"dwm over {wall:.1f} s: {cycles / wall / 1e6:.1f} M cycles/s (CPU time {100 * cpu / wall:.2f}%)  "
          f"GPU 3D {1000 * gpu / wall:.1f} ms/s")
