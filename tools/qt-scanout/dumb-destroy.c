/* Root cause of the CMA leak (docs/FINDINGS-hw-render.md F2): Mesa's renderonly layer (kmsro, used by
 * labwc's GLES2 renderer) imports client dma-bufs into exynos-drm through the RENDER node and later
 * releases them with DRM_IOCTL_MODE_DESTROY_DUMB. The DRM core does not allow that ioctl on render nodes
 * (no DRM_RENDER_ALLOW) so it returns EACCES, the GEM handle is never closed, and the 4.6 MiB CMA buffer
 * stays pinned until the compositor exits. DRM_IOCTL_GEM_CLOSE is allowed on render nodes and does the
 * same thing (drm_gem_handle_delete), so retry as GEM_CLOSE. LD_PRELOAD this into the compositor only.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdarg.h>
#include <stdint.h>
#include <drm/drm.h>
#include <drm/drm_mode.h>

int ioctl(int fd, unsigned long request, ...)
{
    static int (*real)(int, unsigned long, void *);
    va_list ap;
    void *arg;

    if (!real)
        real = (int (*)(int, unsigned long, void *))dlsym(RTLD_NEXT, "ioctl");
    va_start(ap, request);
    arg = va_arg(ap, void *);
    va_end(ap);

    int ret = real(fd, request, arg);
    if (ret == -1 && errno == EACCES && request == DRM_IOCTL_MODE_DESTROY_DUMB && arg) {
        struct drm_gem_close close_req = { .handle = ((struct drm_mode_destroy_dumb *)arg)->handle };
        ret = real(fd, DRM_IOCTL_GEM_CLOSE, &close_req);
    }
    return ret;
}
