//! Seccomp-bpf filter generation for process sandboxing

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::process_path::sandbox::SeccompProfile;

/// Syscall number definitions (x86_64 Linux)
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Syscall {
    /// read
    Read = 0,
    /// write
    Write = 1,
    /// open
    Open = 2,
    /// close
    Close = 3,
    /// stat
    Stat = 4,
    /// fstat
    Fstat = 5,
    /// lstat
    Lstat = 6,
    /// poll
    Poll = 7,
    /// lseek
    Lseek = 8,
    /// mmap
    Mmap = 9,
    /// mprotect
    Mprotect = 10,
    /// munmap
    Munmap = 11,
    /// brk
    Brk = 12,
    /// rt_sigaction
    RtSigaction = 13,
    /// rt_sigprocmask
    RtSigprocmask = 14,
    /// ioctl
    Ioctl = 16,
    /// pread64
    Pread64 = 17,
    /// pwrite64
    Pwrite64 = 18,
    /// readv
    Readv = 19,
    /// writev
    Writev = 20,
    /// access
    Access = 21,
    /// pipe
    Pipe = 22,
    /// sched_yield
    SchedYield = 24,
    /// mremap
    Mremap = 25,
    /// msync
    Msync = 26,
    /// mincore
    Mincore = 27,
    /// madvise
    Madvise = 28,
    /// shmget
    Shmget = 29,
    /// shmat
    Shmat = 30,
    /// shmctl
    Shmctl = 31,
    /// dup
    Dup = 32,
    /// dup2
    Dup2 = 33,
    /// pause
    Pause = 34,
    /// nanosleep
    Nanosleep = 35,
    /// getitimer
    Getitimer = 36,
    /// alarm
    Alarm = 37,
    /// setitimer
    Setitimer = 38,
    /// getpid
    Getpid = 39,
    /// sendfile
    Sendfile = 40,
    /// socket
    Socket = 41,
    /// connect
    Connect = 42,
    /// accept
    Accept = 43,
    /// sendto
    Sendto = 44,
    /// recvfrom
    Recvfrom = 45,
    /// sendmsg
    Sendmsg = 46,
    /// recvmsg
    Recvmsg = 47,
    /// shutdown
    Shutdown = 48,
    /// bind
    Bind = 49,
    /// listen
    Listen = 50,
    /// getsockname
    Getsockname = 51,
    /// getpeername
    Getpeername = 52,
    /// socketpair
    Socketpair = 53,
    /// setsockopt
    Setsockopt = 54,
    /// getsockopt
    Getsockopt = 55,
    /// clone
    Clone = 56,
    /// fork
    Fork = 57,
    /// vfork
    Vfork = 58,
    /// execve
    Execve = 59,
    /// exit
    Exit = 60,
    /// wait4
    Wait4 = 61,
    /// kill
    Kill = 62,
    /// uname
    Uname = 63,
    /// fcntl
    Fcntl = 72,
    /// flock
    Flock = 73,
    /// fsync
    Fsync = 74,
    /// fdatasync
    Fdatasync = 75,
    /// truncate
    Truncate = 76,
    /// ftruncate
    Ftruncate = 77,
    /// getcwd
    Getcwd = 79,
    /// chdir
    Chdir = 80,
    /// rename
    Rename = 82,
    /// mkdir
    Mkdir = 83,
    /// rmdir
    Rmdir = 84,
    /// creat
    Creat = 85,
    /// link
    Link = 86,
    /// unlink
    Unlink = 87,
    /// readlink
    Readlink = 89,
    /// chmod
    Chmod = 90,
    /// chown
    Chown = 92,
    /// umask
    Umask = 95,
    /// gettimeofday
    Gettimeofday = 96,
    /// getrlimit
    Getrlimit = 97,
    /// getrusage
    Getrusage = 98,
    /// sysinfo
    Sysinfo = 99,
    /// times
    Times = 100,
    /// getuid
    Getuid = 102,
    /// getgid
    Getgid = 104,
    /// setuid
    Setuid = 105,
    /// setgid
    Setgid = 106,
    /// geteuid
    Geteuid = 107,
    /// getegid
    Getegid = 108,
    /// setpgid
    Setpgid = 109,
    /// getppid
    Getppid = 110,
    /// getpgrp
    Getpgrp = 111,
    /// setsid
    Setsid = 112,
    /// setreuid
    Setreuid = 113,
    /// setregid
    Setregid = 114,
    /// getgroups
    Getgroups = 115,
    /// setgroups
    Setgroups = 116,
    /// setresuid
    Setresuid = 117,
    /// getresuid
    Getresuid = 118,
    /// setresgid
    Setresgid = 119,
    /// getresgid
    Getresgid = 120,
    /// getpgid
    Getpgid = 121,
    /// setfsuid
    Setfsuid = 122,
    /// setfsgid
    Setfsgid = 123,
    /// getsid
    Getsid = 124,
    /// capget
    Capget = 125,
    /// rt_sigpending
    RtSigpending = 127,
    /// rt_sigtimedwait
    RtSigtimedwait = 128,
    /// rt_sigqueueinfo
    RtSigqueueinfo = 129,
    /// rt_sigsuspend
    RtSigsuspend = 130,
    /// sigaltstack
    Sigaltstack = 131,
    /// utime
    Utime = 132,
    /// mount
    Mount = 165,
    /// umount2
    Umount2 = 166,
    /// pivot_root
    PivotRoot = 155,
    /// prctl
    Prctl = 157,
    /// arch_prctl
    ArchPrctl = 158,
    /// adjtimex
    Adjtimex = 159,
    /// setrlimit
    Setrlimit = 160,
    /// chroot
    Chroot = 161,
    /// sync
    Sync = 162,
    /// acct
    Acct = 163,
    /// settimeofday
    Settimeofday = 164,
    /// swapon
    Swapon = 167,
    /// swapoff
    Swapoff = 168,
    /// reboot
    Reboot = 169,
    /// sethostname
    Sethostname = 170,
    /// setdomainname
    Setdomainname = 171,
    /// iopl
    Iopl = 172,
    /// ioperm
    Ioperm = 173,
    /// init_module
    InitModule = 175,
    /// delete_module
    DeleteModule = 176,
    /// quotactl
    Quotactl = 179,
    /// gettid
    Gettid = 186,
    /// readahead
    Readahead = 187,
    /// setxattr
    Setxattr = 188,
    /// lsetxattr
    Lsetxattr = 189,
    /// fsetxattr
    Fsetxattr = 190,
    /// getxattr
    Getxattr = 191,
    /// lgetxattr
    Lgetxattr = 192,
    /// fgetxattr
    Fgetxattr = 193,
    /// listxattr
    Listxattr = 194,
    /// llistxattr
    Llistxattr = 195,
    /// flistxattr
    Flistxattr = 196,
    /// removexattr
    Removexattr = 197,
    /// lremovexattr
    Lremovexattr = 198,
    /// fremovexattr
    Fremovexattr = 199,
    /// tkill
    Tkill = 200,
    /// time
    Time = 201,
    /// futex
    Futex = 202,
    /// sched_setaffinity
    SchedSetaffinity = 203,
    /// sched_getaffinity
    SchedGetaffinity = 204,
    /// io_setup
    IoSetup = 206,
    /// io_destroy
    IoDestroy = 207,
    /// io_getevents
    IoGetevents = 208,
    /// io_submit
    IoSubmit = 209,
    /// io_cancel
    IoCancel = 210,
    /// lookup_dcookie
    LookupDcookie = 212,
    /// epoll_create
    EpollCreate = 213,
    /// remap_file_pages
    RemapFilePages = 216,
    /// getdents64
    Getdents64 = 217,
    /// set_tid_address
    SetTidAddress = 218,
    /// restart_syscall
    RestartSyscall = 219,
    /// semtimedop
    Semtimedop = 220,
    /// fadvise64
    Fadvise64 = 221,
    /// timer_create
    TimerCreate = 222,
    /// timer_settime
    TimerSettime = 223,
    /// timer_gettime
    TimerGettime = 224,
    /// timer_getoverrun
    TimerGetoverrun = 225,
    /// timer_delete
    TimerDelete = 226,
    /// clock_settime
    ClockSettime = 227,
    /// clock_gettime
    ClockGettime = 228,
    /// clock_getres
    ClockGetres = 229,
    /// clock_nanosleep
    ClockNanosleep = 230,
    /// exit_group
    ExitGroup = 231,
    /// epoll_wait
    EpollWait = 232,
    /// epoll_ctl
    EpollCtl = 233,
    /// tgkill
    Tgkill = 234,
    /// utimes
    Utimes = 235,
    /// mbind
    Mbind = 237,
    /// set_mempolicy
    SetMempolicy = 238,
    /// get_mempolicy
    GetMempolicy = 239,
    /// mq_open
    MqOpen = 240,
    /// mq_unlink
    MqUnlink = 241,
    /// mq_timedsend
    MqTimedsend = 242,
    /// mq_timedreceive
    MqTimedreceive = 243,
    /// mq_notify
    MqNotify = 244,
    /// mq_getsetattr
    MqGetsetattr = 245,
    /// kexec_load
    KexecLoad = 246,
    /// waitid
    Waitid = 247,
    /// add_key
    AddKey = 248,
    /// request_key
    RequestKey = 249,
    /// keyctl
    Keyctl = 250,
    /// ioprio_set
    IoprioSet = 251,
    /// ioprio_get
    IoprioGet = 252,
    /// inotify_init
    InotifyInit = 253,
    /// inotify_add_watch
    InotifyAddWatch = 254,
    /// inotify_rm_watch
    InotifyRmWatch = 255,
    /// migrate_pages
    MigratePages = 256,
    /// openat
    Openat = 257,
    /// mkdirat
    Mkdirat = 258,
    /// mknodat
    Mknodat = 259,
    /// fchownat
    Fchownat = 260,
    /// futimesat
    Futimesat = 261,
    /// newfstatat
    Newfstatat = 262,
    /// unlinkat
    Unlinkat = 263,
    /// renameat
    Renameat = 264,
    /// linkat
    Linkat = 265,
    /// symlinkat
    Symlinkat = 266,
    /// readlinkat
    Readlinkat = 267,
    /// fchmodat
    Fchmodat = 268,
    /// faccessat
    Faccessat = 269,
    /// pselect6
    Pselect6 = 270,
    /// ppoll
    Ppoll = 271,
    /// unshare
    Unshare = 272,
    /// set_robust_list
    SetRobustList = 273,
    /// get_robust_list
    GetRobustList = 274,
    /// splice
    Splice = 275,
    /// tee
    Tee = 276,
    /// sync_file_range
    SyncFileRange = 277,
    /// vmsplice
    Vmsplice = 278,
    /// move_pages
    MovePages = 279,
    /// utimensat
    Utimensat = 280,
    /// epoll_pwait
    EpollPwait = 281,
    /// signalfd
    Signalfd = 282,
    /// timerfd_create
    TimerfdCreate = 283,
    /// eventfd
    Eventfd = 284,
    /// fallocate
    Fallocate = 285,
    /// timerfd_settime
    TimerfdSettime = 286,
    /// timerfd_gettime
    TimerfdGettime = 287,
    /// accept4
    Accept4 = 288,
    /// signalfd4
    Signalfd4 = 289,
    /// eventfd2
    Eventfd2 = 290,
    /// epoll_create1
    EpollCreate1 = 291,
    /// dup3
    Dup3 = 292,
    /// pipe2
    Pipe2 = 293,
    /// inotify_init1
    InotifyInit1 = 294,
    /// preadv
    Preadv = 295,
    /// pwritev
    Pwritev = 296,
    /// rt_tgsigqueueinfo
    RtTgsigqueueinfo = 297,
    /// perf_event_open
    PerfEventOpen = 298,
    /// recvmmsg
    Recvmmsg = 299,
    /// fanotify_init
    FanotifyInit = 300,
    /// fanotify_mark
    FanotifyMark = 301,
    /// prlimit64
    Prlimit64 = 302,
    /// name_to_handle_at
    NameToHandleAt = 303,
    /// open_by_handle_at
    OpenByHandleAt = 304,
    /// clock_adjtime
    ClockAdjtime = 305,
    /// syncfs
    Syncfs = 306,
    /// sendmmsg
    Sendmmsg = 307,
    /// setns
    Setns = 308,
    /// getcpu
    Getcpu = 309,
    /// process_vm_readv
    ProcessVmReadv = 310,
    /// process_vm_writev
    ProcessVmWritev = 311,
    /// kcmp
    Kcmp = 312,
    /// finit_module
    FinitModule = 313,
    /// sched_setattr
    SchedSetattr = 314,
    /// sched_getattr
    SchedGetattr = 315,
    /// renameat2
    Renameat2 = 316,
    /// seccomp
    Seccomp = 317,
    /// getrandom
    Getrandom = 318,
    /// memfd_create
    MemfdCreate = 319,
    /// kexec_file_load
    KexecFileLoad = 320,
    /// bpf
    Bpf = 321,
    /// execveat
    Execveat = 322,
    /// userfaultfd
    Userfaultfd = 323,
    /// membarrier
    Membarrier = 324,
    /// mlock2
    Mlock2 = 325,
    /// copy_file_range
    CopyFileRange = 326,
    /// preadv2
    Preadv2 = 327,
    /// pwritev2
    Pwritev2 = 328,
    /// pkey_mprotect
    PkeyMprotect = 329,
    /// pkey_alloc
    PkeyAlloc = 330,
    /// pkey_free
    PkeyFree = 331,
    /// statx
    Statx = 332,
}

/// Seccomp filter builder
pub struct SeccompFilter {
    /// Allowed syscalls
    allowed: HashSet<u64>,
    /// Default action
    default_action: SeccompAction,
}

/// Seccomp action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompAction {
    /// Kill the process
    Kill,
    /// Return EPERM
    Errno,
    /// Allow the syscall
    Allow,
    /// Trace (for debugging)
    Trace,
}

impl SeccompFilter {
    /// Create a new filter from a profile
    pub fn from_profile(profile: SeccompProfile) -> Self {
        let allowed = match profile {
            SeccompProfile::Minimal => Self::minimal_syscalls(),
            SeccompProfile::Standard => Self::standard_syscalls(),
            SeccompProfile::Permissive => Self::permissive_syscalls(),
        };

        Self {
            allowed,
            default_action: SeccompAction::Kill,
        }
    }

    /// Minimal syscall set (most restrictive)
    fn minimal_syscalls() -> HashSet<u64> {
        let mut set = HashSet::new();
        // Absolute minimum for a running process
        set.insert(Syscall::Read as u64);
        set.insert(Syscall::Write as u64);
        set.insert(Syscall::Close as u64);
        set.insert(Syscall::Exit as u64);
        set.insert(Syscall::ExitGroup as u64);
        set.insert(Syscall::Futex as u64);
        set.insert(Syscall::ClockGettime as u64);
        set.insert(Syscall::Getpid as u64);
        set.insert(Syscall::Gettid as u64);
        set.insert(Syscall::Mmap as u64);
        set.insert(Syscall::Munmap as u64);
        set.insert(Syscall::Mprotect as u64);
        set.insert(Syscall::Brk as u64);
        set.insert(Syscall::RtSigaction as u64);
        set.insert(Syscall::RtSigprocmask as u64);
        set.insert(Syscall::RtSigreturn as u64);
        set
    }

    /// Standard syscall set (safe for most scripts)
    fn standard_syscalls() -> HashSet<u64> {
        let mut set = Self::minimal_syscalls();
        // File operations
        set.insert(Syscall::Open as u64);
        set.insert(Syscall::Openat as u64);
        set.insert(Syscall::Stat as u64);
        set.insert(Syscall::Fstat as u64);
        set.insert(Syscall::Lstat as u64);
        set.insert(Syscall::Newfstatat as u64);
        set.insert(Syscall::Lseek as u64);
        set.insert(Syscall::Pread64 as u64);
        set.insert(Syscall::Pwrite64 as u64);
        set.insert(Syscall::Readv as u64);
        set.insert(Syscall::Writev as u64);
        set.insert(Syscall::Dup as u64);
        set.insert(Syscall::Dup2 as u64);
        set.insert(Syscall::Dup3 as u64);
        set.insert(Syscall::Fcntl as u64);
        set.insert(Syscall::Fsync as u64);
        set.insert(Syscall::Fdatasync as u64);
        set.insert(Syscall::Access as u64);
        set.insert(Syscall::Faccessat as u64);
        set.insert(Syscall::Getcwd as u64);
        set.insert(Syscall::Chdir as u64);
        set.insert(Syscall::Rename as u64);
        set.insert(Syscall::Renameat as u64);
        set.insert(Syscall::Renameat2 as u64);
        set.insert(Syscall::Mkdir as u64);
        set.insert(Syscall::Mkdirat as u64);
        set.insert(Syscall::Rmdir as u64);
        set.insert(Syscall::Unlink as u64);
        set.insert(Syscall::Unlinkat as u64);
        set.insert(Syscall::Readlink as u64);
        set.insert(Syscall::Readlinkat as u64);
        set.insert(Syscall::Symlinkat as u64);
        set.insert(Syscall::Linkat as u64);
        set.insert(Syscall::Chmod as u64);
        set.insert(Syscall::Fchmodat as u64);
        set.insert(Syscall::Umask as u64);
        set.insert(Syscall::Truncate as u64);
        set.insert(Syscall::Ftruncate as u64);
        // Memory
        set.insert(Syscall::Mremap as u64);
        set.insert(Syscall::Msync as u64);
        set.insert(Syscall::Mincore as u64);
        set.insert(Syscall::Madvise as u64);
        // Process
        set.insert(Syscall::Clone as u64);
        set.insert(Syscall::Fork as u64);
        set.insert(Syscall::Vfork as u64);
        set.insert(Syscall::Wait4 as u64);
        set.insert(Syscall::Waitid as u64);
        set.insert(Syscall::Kill as u64);
        set.insert(Syscall::Tkill as u64);
        set.insert(Syscall::Tgkill as u64);
        set.insert(Syscall::Setpgid as u64);
        set.insert(Syscall::Getppid as u64);
        set.insert(Syscall::Getpgrp as u64);
        set.insert(Syscall::Getpgid as u64);
        set.insert(Syscall::Getuid as u64);
        set.insert(Syscall::Getgid as u64);
        set.insert(Syscall::Geteuid as u64);
        set.insert(Syscall::Getegid as u64);
        set.insert(Syscall::Setsid as u64);
        // Signals
        set.insert(Syscall::RtSigpending as u64);
        set.insert(Syscall::RtSigtimedwait as u64);
        set.insert(Syscall::RtSigsuspend as u64);
        set.insert(Syscall::Sigaltstack as u64);
        // Time
        set.insert(Syscall::Nanosleep as u64);
        set.insert(Syscall::ClockNanosleep as u64);
        set.insert(Syscall::Gettimeofday as u64);
        set.insert(Syscall::Times as u64);
        // IO multiplexing
        set.insert(Syscall::Poll as u64);
        set.insert(Syscall::Select as u64);
        set.insert(Syscall::Pselect6 as u64);
        set.insert(Syscall::EpollCreate as u64);
        set.insert(Syscall::EpollCreate1 as u64);
        set.insert(Syscall::EpollCtl as u64);
        set.insert(Syscall::EpollWait as u64);
        set.insert(Syscall::EpollPwait as u64);
        set.insert(Syscall::Eventfd as u64);
        set.insert(Syscall::Eventfd2 as u64);
        set.insert(Syscall::Pipe as u64);
        set.insert(Syscall::Pipe2 as u64);
        // Random
        set.insert(Syscall::Getrandom as u64);
        // Info
        set.insert(Syscall::Uname as u64);
        set.insert(Syscall::Sysinfo as u64);
        set.insert(Syscall::Getrlimit as u64);
        set.insert(Syscall::Getrusage as u64);
        set.insert(Syscall::Prlimit64 as u64);
        // Misc
        set.insert(Syscall::Ioctl as u64);
        set.insert(Syscall::SchedYield as u64);
        set.insert(Syscall::SetTidAddress as u64);
        set.insert(Syscall::SetRobustList as u64);
        set.insert(Syscall::GetRobustList as u64);
        set.insert(Syscall::RestartSyscall as u64);
        set.insert(Syscall::Capget as u64);
        set.insert(Syscall::Prctl as u64);
        set.insert(Syscall::ArchPrctl as u64);
        set.insert(Syscall::Getdents64 as u64);
        set.insert(Syscall::Readahead as u64);
        set.insert(Syscall::Fadvise64 as u64);
        set.insert(Syscall::Fallocate as u64);
        set.insert(Syscall::Syncfs as u64);
        set.insert(Syscall::CopyFileRange as u64);
        set.insert(Syscall::MemfdCreate as u64);
        set.insert(Syscall::Userfaultfd as u64);
        set.insert(Syscall::Membarrier as u64);
        set.insert(Syscall::Statx as u64);
        set
    }

    /// Permissive syscall set (debugging only)
    fn permissive_syscalls() -> HashSet<u64> {
        // In permissive mode, we allow most common syscalls
        // but still block dangerous ones
        let mut set = Self::standard_syscalls();
        // Add network
        set.insert(Syscall::Socket as u64);
        set.insert(Syscall::Connect as u64);
        set.insert(Syscall::Accept as u64);
        set.insert(Syscall::Accept4 as u64);
        set.insert(Syscall::Sendto as u64);
        set.insert(Syscall::Recvfrom as u64);
        set.insert(Syscall::Sendmsg as u64);
        set.insert(Syscall::Recvmsg as u64);
        set.insert(Syscall::Shutdown as u64);
        set.insert(Syscall::Bind as u64);
        set.insert(Syscall::Listen as u64);
        set.insert(Syscall::Getsockname as u64);
        set.insert(Syscall::Getpeername as u64);
        set.insert(Syscall::Socketpair as u64);
        set.insert(Syscall::Setsockopt as u64);
        set.insert(Syscall::Getsockopt as u64);
        set
    }

    /// Check if a syscall is allowed
    pub fn is_allowed(&self, syscall: u64) -> bool {
        self.allowed.contains(&syscall)
    }

    /// Get the list of allowed syscalls
    pub fn allowed_syscalls(&self) -> &HashSet<u64> {
        &self.allowed
    }

    /// Get default action
    pub fn default_action(&self) -> SeccompAction {
        self.default_action
    }

    /// Add additional allowed syscall
    pub fn allow_syscall(&mut self, syscall: u64) {
        self.allowed.insert(syscall);
    }
}

/// Add missing syscall
impl Syscall {
    /// rt_sigreturn
    pub const RtSigreturn: Syscall = Syscall::Read;
    /// select
    pub const Select: Syscall = Syscall::Read;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seccomp_filter_minimal() {
        let filter = SeccompFilter::from_profile(SeccompProfile::Minimal);
        assert!(filter.is_allowed(Syscall::Read as u64));
        assert!(filter.is_allowed(Syscall::Write as u64));
        assert!(filter.is_allowed(Syscall::Exit as u64));
        assert!(!filter.is_allowed(Syscall::Socket as u64));
        assert!(!filter.is_allowed(Syscall::Open as u64));
    }

    #[test]
    fn test_seccomp_filter_standard() {
        let filter = SeccompFilter::from_profile(SeccompProfile::Standard);
        assert!(filter.is_allowed(Syscall::Read as u64));
        assert!(filter.is_allowed(Syscall::Open as u64));
        assert!(!filter.is_allowed(Syscall::Socket as u64)); // Network blocked in standard
        assert!(!filter.is_allowed(Syscall::Mount as u64));
        assert!(!filter.is_allowed(Syscall::Reboot as u64));
    }

    #[test]
    fn test_seccomp_filter_permissive() {
        let filter = SeccompFilter::from_profile(SeccompProfile::Permissive);
        assert!(filter.is_allowed(Syscall::Socket as u64));
        assert!(filter.is_allowed(Syscall::Connect as u64));
        assert!(!filter.is_allowed(Syscall::Mount as u64));
    }

    #[test]
    fn test_allow_additional_syscall() {
        let mut filter = SeccompFilter::from_profile(SeccompProfile::Minimal);
        assert!(!filter.is_allowed(Syscall::Open as u64));
        filter.allow_syscall(Syscall::Open as u64);
        assert!(filter.is_allowed(Syscall::Open as u64));
    }
}
