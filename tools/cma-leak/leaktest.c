// leaktest: isolate where exynos-drm CMA buffers leak.
//  mode 0: CREATE_DUMB on card0, close handle, exit              (kernel dumb alloc/free)
//  mode 1: CREATE_DUMB, export dma-buf, close fd+handle          (prime export)
//  mode 2: mode 1, import the dma-buf into a SECOND drm file (card0 reopened), close both handles
//  mode 4: export from card0, import into NODE2 (Panfrost render node), close handle (GPU-side import path)
//  mode 3: mode 2 but the importer leaves its handle open until process exit (== compositor behaviour)
// Build: gcc -O2 -o leaktest leaktest.c   Run as a user that may open card0, N times, watch CmaFree.
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/ioctl.h>
#include <stdint.h>
#include <drm/drm.h>
#include <drm/drm_mode.h>
int main(int argc, char **argv) {
    int mode = argc > 1 ? atoi(argv[1]) : 0, n = argc > 2 ? atoi(argv[2]) : 3;
    const char *node = getenv("NODE") ? getenv("NODE") : "/dev/dri/card0";
    int fd = open(node, O_RDWR | O_CLOEXEC), fd2 = -1;
    if (fd < 0) { perror("open"); return 1; }
    for (int i = 0; i < n; i++) {
        struct drm_mode_create_dumb c = { .width = 720, .height = 1659, .bpp = 32 };
        if (ioctl(fd, DRM_IOCTL_MODE_CREATE_DUMB, &c)) { perror("create_dumb"); return 1; }
        if (mode >= 1) {
            struct drm_prime_handle p = { .handle = c.handle, .flags = DRM_CLOEXEC };
            if (ioctl(fd, DRM_IOCTL_PRIME_HANDLE_TO_FD, &p)) { perror("h2fd"); return 1; }
            if (mode == 4) {
                const char *n2 = getenv("NODE2") ? getenv("NODE2") : "/dev/dri/renderD129";
                if (fd2 < 0) fd2 = open(n2, O_RDWR | O_CLOEXEC);
                struct drm_prime_handle q = { .fd = p.fd };
                if (ioctl(fd2, DRM_IOCTL_PRIME_FD_TO_HANDLE, &q)) perror("pan fd2h");
                struct drm_gem_close g = { .handle = q.handle }; ioctl(fd2, DRM_IOCTL_GEM_CLOSE, &g);
            }
            if (mode >= 2 && mode <= 3) {
                if (fd2 < 0) fd2 = open(node, O_RDWR | O_CLOEXEC);
                struct drm_prime_handle q = { .fd = p.fd };
                if (ioctl(fd2, DRM_IOCTL_PRIME_FD_TO_HANDLE, &q)) perror("fd2h");
                if (mode == 2) { struct drm_gem_close g = { .handle = q.handle }; ioctl(fd2, DRM_IOCTL_GEM_CLOSE, &g); }
            }
            close(p.fd);
        }
        struct drm_mode_destroy_dumb d = { .handle = c.handle };
        ioctl(fd, DRM_IOCTL_MODE_DESTROY_DUMB, &d);
    }
    return 0;   // fds closed by exit
}
