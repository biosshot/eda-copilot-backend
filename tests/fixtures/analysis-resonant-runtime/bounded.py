"""Windows job supervisor for one runtime coupon (including worker children).
512 MiB process + aggregate job commit, 60 s per-process/job CPU and wall.
No fallback on supervisor failure. Compilation is deliberately outside this job.
Usage: python bounded.py META.json EXECUTABLE [ARG ...]
"""
import ctypes,json,os,pathlib,subprocess,sys,time
from ctypes import wintypes as w
if os.name!='nt':raise RuntimeError('Windows job required')
class Basic(ctypes.Structure):
    _fields_=[('processTime',ctypes.c_longlong),('jobTime',ctypes.c_longlong),('flags',w.DWORD),
        ('minWs',ctypes.c_size_t),('maxWs',ctypes.c_size_t),('active',w.DWORD),('affinity',ctypes.c_size_t),('priority',w.DWORD),('scheduling',w.DWORD)]
class Extended(ctypes.Structure):
    _fields_=[('basic',Basic),('io',ctypes.c_ulonglong*6),('processMemory',ctypes.c_size_t),('jobMemory',ctypes.c_size_t),('peakProcess',ctypes.c_size_t),('peakJob',ctypes.c_size_t)]
class Accounting(ctypes.Structure):
    _fields_=[('user',ctypes.c_longlong),('kernel',ctypes.c_longlong),('periodUser',ctypes.c_longlong),('periodKernel',ctypes.c_longlong),
        ('faults',w.DWORD),('totalProcesses',w.DWORD),('activeProcesses',w.DWORD),('terminatedProcesses',w.DWORD)]
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.CreateJobObjectW.argtypes=[ctypes.c_void_p,w.LPCWSTR];k.CreateJobObjectW.restype=w.HANDLE
k.GetCurrentProcess.restype=w.HANDLE
k.SetInformationJobObject.argtypes=[w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD]
k.AssignProcessToJobObject.argtypes=[w.HANDLE,w.HANDLE]
k.QueryInformationJobObject.argtypes=[w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD,ctypes.c_void_p]
k.TerminateJobObject.argtypes=[w.HANDLE,w.UINT]
job=k.CreateJobObjectW(None,None);limit=Extended()
limit.basic.flags=0x4|0x2|0x100|0x200|0x2000
limit.basic.processTime=limit.basic.jobTime=60*10000000
limit.processMemory=limit.jobMemory=512*1048576
if not job or not k.SetInformationJobObject(job,9,ctypes.byref(limit),ctypes.sizeof(limit)) or not k.AssignProcessToJobObject(job,k.GetCurrentProcess()):raise ctypes.WinError(ctypes.get_last_error())
start=time.perf_counter();process=subprocess.Popen(sys.argv[2:],stdout=sys.stdout,stderr=sys.stderr,creationflags=subprocess.CREATE_NO_WINDOW)
try:code=process.wait(timeout=60)
except subprocess.TimeoutExpired:k.TerminateJobObject(job,124);raise
extended=Extended();account=Accounting()
if not k.QueryInformationJobObject(job,9,ctypes.byref(extended),ctypes.sizeof(extended),None) or not k.QueryInformationJobObject(job,1,ctypes.byref(account),ctypes.sizeof(account),None):raise ctypes.WinError(ctypes.get_last_error())
metadata={'command':sys.argv[2:],'exitCode':code,'wallSeconds':time.perf_counter()-start,
    'jobUserCpuSeconds':account.user/1e7,'jobKernelCpuSeconds':account.kernel/1e7,
    'peakProcessCommitBytes':extended.peakProcess,'peakJobCommitBytes':extended.peakJob,'totalProcesses':account.totalProcesses,
    'hardProcessMiB':512,'hardJobMiB':512,'hardCpuSeconds':60,'hardWallSeconds':60}
p=pathlib.Path(sys.argv[1]);p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(metadata,indent=2)+'\n')
print(json.dumps(metadata));sys.exit(code)
