// The gate's runs of a binary count the same instructions every time
// (gate.py preloads this into each, under valgrind; never into anything
// shipped). Without it the same binary counted up to 0.3% apart:
//
// - CLOCK_MONOTONIC: Luau's collector paces its cycles by measured
//   allocation rates and mark times, and a script call reads the thread's
//   CPU time only when its last reading is over a millisecond old. Here
//   the clock starts at the whole second after the process did (real
//   enough that a futex deadline made from it, std's park_timeout, is in
//   the future), and only the main thread moves it: `STEP` a read, and
//   what it sleeps. Other threads (the timer scheduler, the script
//   watchdog) read where it is. `STEP` is app-mail's native time between
//   the main thread's reads (26-29 us, measured without valgrind), so a
//   heuristic paced by reads (the CPU-time reading's age) runs as natively.
//   Luau's heap trigger compares a cycle's mark time with its allocation
//   time, which a clock moved by reads cannot measure (marking reads it
//   about never): here cycles start at the heap goal, and app-mail does
//   about 1% less collecting than on the real clock, at any step. To
//   measure a change to that pacing, count on the real clock:
//   `ZGX_REAL_CLOCK=1 python3 scripts/perf/gate.py BASE` (then exact to
//   about 0.3% in the Luau scenes).
// - Futex deadlines: std's park_timeout and its condition variables wait
//   until an absolute CLOCK_MONOTONIC time made from this clock, which a
//   long run under valgrind leaves behind real time: waits came back at
//   once, and the script watchdog spun until the main thread moved on, as
//   many times as the scheduling made (micro-vscroll-variable-luau's count
//   swung 139.9-141.5 M). Here such a wait lasts, in real time, as long
//   from now as it asked of this clock.
// - CLOCK_PROCESS_CPUTIME_ID: the micro benches time their batches by it,
//   sort what they read and print it, and formatting a float costs what its
//   digits do (one binary's micro-state moved 7,000 instructions, 0.01%, in
//   `flt2dec`). Here each read is `STEP` on from the last.
// - getrandom: std's RandomState seeds every HashMap from it, and a map's
//   probes and growth move with the seed. Here it gives the same bytes.
//
// With ZGX_SAME_RUN_REPORT set, the reads and calls are written to stderr
// as the process exits: gate.py checks this was loaded.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <linux/futex.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

// libc's syscall, for this file's own: `syscall` below is the program's.
static long (*libc_syscall)(long, ...);
static long raw(long n, long a, long b, long c, long d, long e, long f) {
    if (!libc_syscall)
        libc_syscall = (long (*)(long, ...))dlsym(RTLD_NEXT, "syscall");
    return libc_syscall(n, a, b, c, d, e, f);
}

#define STEP 25000ULL

static unsigned long long now, cpu;
static int real_clock;
static unsigned long clock_reads, random_calls, futex_waits;

static void report(void) {
    if (getenv("ZGX_SAME_RUN_REPORT"))
        fprintf(stderr, "same_run: %lu clock reads, %lu getrandom calls, %lu futex waits\n", clock_reads,
                random_calls, futex_waits);
}

__attribute__((constructor)) static void init(void) {
    struct timespec real;
    raw(SYS_clock_gettime, CLOCK_MONOTONIC, (long)&real, 0, 0, 0, 0);
    now = ((unsigned long long)real.tv_sec + 1) * 1000000000ULL;
    real_clock = getenv("ZGX_REAL_CLOCK") != NULL;
    atexit(report);
}

int clock_gettime(clockid_t clock, struct timespec *tp) {
    if (clock == CLOCK_PROCESS_CPUTIME_ID && !real_clock) {
        unsigned long long ns = __atomic_add_fetch(&cpu, STEP, __ATOMIC_SEQ_CST);
        tp->tv_sec = (time_t)(ns / 1000000000ULL);
        tp->tv_nsec = (long)(ns % 1000000000ULL);
        return 0;
    }
    if (clock != CLOCK_MONOTONIC || real_clock)
        return (int)raw(SYS_clock_gettime, clock, (long)tp, 0, 0, 0, 0);
    __atomic_fetch_add(&clock_reads, 1, __ATOMIC_RELAXED);
    // Only the main thread moves it: what other threads read (a timer's
    // or the script watchdog's) never changes what it reads next.
    unsigned long long ns = raw(SYS_gettid, 0, 0, 0, 0, 0, 0) == getpid()
        ? __atomic_add_fetch(&now, STEP, __ATOMIC_SEQ_CST)
        : __atomic_load_n(&now, __ATOMIC_SEQ_CST);
    tp->tv_sec = (time_t)(ns / 1000000000ULL);
    tp->tv_nsec = (long)(ns % 1000000000ULL);
    return 0;
}

// The main thread's sleeps move its clock on by what they asked for, as
// time would: a test app waiting a millisecond at a time for a timer
// another thread fires sees it pass.
static void slept(const struct timespec *asked) {
    if (raw(SYS_gettid, 0, 0, 0, 0, 0, 0) == getpid())
        __atomic_add_fetch(&now, (unsigned long long)asked->tv_sec * 1000000000ULL + (unsigned long long)asked->tv_nsec,
                           __ATOMIC_SEQ_CST);
}

int nanosleep(const struct timespec *asked, struct timespec *left) {
    int r = (int)raw(SYS_nanosleep, (long)asked, (long)left, 0, 0, 0, 0);
    slept(asked);
    return r;
}

int clock_nanosleep(clockid_t clock, int flags, const struct timespec *asked, struct timespec *left) {
    if ((clock != CLOCK_MONOTONIC && clock != CLOCK_REALTIME) || flags != 0)
        return (int)raw(SYS_clock_nanosleep, clock, flags, (long)asked, (long)left, 0, 0);
    int r = (int)raw(SYS_clock_nanosleep, clock, flags, (long)asked, (long)left, 0, 0);
    slept(asked);
    return r;
}

// A futex wait until a CLOCK_MONOTONIC time: as long from the real now as
// from this clock's.
long syscall(long n, ...) {
    va_list ap;
    va_start(ap, n);
    long a[6];
    for (int i = 0; i < 6; i++)
        a[i] = va_arg(ap, long);
    va_end(ap);
    int op = (int)a[1];
    struct timespec until;
    if (n == SYS_futex && !real_clock && a[3] && (op & FUTEX_CMD_MASK) == FUTEX_WAIT_BITSET &&
        !(op & FUTEX_CLOCK_REALTIME)) {
        __atomic_fetch_add(&futex_waits, 1, __ATOMIC_RELAXED);
        const struct timespec *asked = (const struct timespec *)a[3];
        long long left = ((long long)asked->tv_sec * 1000000000LL + asked->tv_nsec) -
                         (long long)__atomic_load_n(&now, __ATOMIC_SEQ_CST);
        struct timespec real;
        raw(SYS_clock_gettime, CLOCK_MONOTONIC, (long)&real, 0, 0, 0, 0);
        long long at = (long long)real.tv_sec * 1000000000LL + real.tv_nsec + (left > 0 ? left : 0);
        until.tv_sec = (time_t)(at / 1000000000LL);
        until.tv_nsec = (long)(at % 1000000000LL);
        a[3] = (long)&until;
    }
    return raw(n, a[0], a[1], a[2], a[3], a[4], a[5]);
}

ssize_t getrandom(void *buf, size_t len, unsigned int flags) {
    (void)flags;
    __atomic_fetch_add(&random_calls, 1, __ATOMIC_RELAXED);
    unsigned char *out = buf;
    for (size_t i = 0; i < len; i++)
        out[i] = (unsigned char)(i * 131 + 7);
    return (ssize_t)len;
}
