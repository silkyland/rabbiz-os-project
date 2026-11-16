/* Mesa kmsro must allocate dumb buffers on the primary node, not a render node.
 * Process-local experiment; node paths are resolved by a21s-qt-gpu.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>

static int kms_open(const char *path, int flags, mode_t mode)
{
    int (*real_open)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    if (!real_open) {
        errno = ENOSYS;
        return -1;
    }
    const char *render = getenv("RABBIZ_EXYNOS_RENDER");
    const char *primary = getenv("RABBIZ_EXYNOS_PRIMARY");
    if (render && primary && strcmp(path, render) == 0)
        path = primary;
    return real_open(path, flags, mode);
}

int open64(const char *path, int flags, ...)
{
    mode_t mode = 0;
    if ((flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE)) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, int);
        va_end(args);
    }
    return kms_open(path, flags, mode);
}

int open(const char *path, int flags, ...)
{
    mode_t mode = 0;
    if ((flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE)) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, int);
        va_end(args);
    }
    return kms_open(path, flags, mode);
}
