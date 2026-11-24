/* The ES-renderer write hook for rust-modules/src/player/es_carrier.rs.
 *
 * LG's libkadaptor runs in this process and writes IEC 61937 bursts into its ES renderer with
 * ioctl(fd, 0x40285207, arg). The executable exports this ioctl (Makefile: --export-dynamic-symbol),
 * so that call binds here; every call is forwarded with the raw syscall, and an ES-renderer write
 * (word 8 == 1) is first offered to nj_es_swap, which may point word 0 at a queued burst. */
#include <stdarg.h>
#include <sys/syscall.h>
#include <unistd.h>

void nj_es_swap(unsigned *w);
void nj_es_swap_done(const unsigned *w, int ret);

int ioctl(int fd, unsigned long req, ...) {
    va_list ap;
    va_start(ap, req);
    void *arg = va_arg(ap, void *);
    va_end(ap);
    int es = req == 0x40285207UL && arg && ((unsigned *)arg)[8] == 1;
    if (es)
        nj_es_swap((unsigned *)arg);
    int ret = (int)syscall(SYS_ioctl, fd, req, arg);
    if (es)
        nj_es_swap_done((const unsigned *)arg, ret);
    return ret;
}
