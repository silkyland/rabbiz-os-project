#ifndef _SCSC_COMPAT_MAINLINE_H
#define _SCSC_COMPAT_MAINLINE_H

#include <linux/version.h>
#include <linux/time64.h>
#include <linux/ktime.h>
#include <linux/bitmap.h>
#include <linux/string.h>
#include <linux/proc_fs.h>
#include <linux/vmalloc.h>
#include <linux/io.h>
#include <linux/umh.h>
#include <linux/arm-smccc.h>

#ifndef CONFIG_SCSC_CORE_FW_LOCATION
#define CONFIG_SCSC_CORE_FW_LOCATION "/vendor/etc/wifi"
#endif
#ifndef CONFIG_SCSC_CORE_TOOL_LOCATION
#define CONFIG_SCSC_CORE_TOOL_LOCATION "/vendor/bin"
#endif
#ifndef CONFIG_SCSC_WLAN_MAX_INTERFACES
#define CONFIG_SCSC_WLAN_MAX_INTERFACES 3
#endif

#ifndef PDE_DATA
#define PDE_DATA(inode) pde_data(inode)
#endif

#ifndef devm_ioremap_nocache
#define devm_ioremap_nocache devm_ioremap
#endif

#ifndef strlcpy
#define strlcpy strscpy
#endif

#ifndef timeval
struct compat_timeval {
	long tv_sec;
	long tv_usec;
};
#define timeval compat_timeval
static inline struct compat_timeval ns_to_timeval(const s64 nsec)
{
	struct timespec64 ts = ns_to_timespec64(nsec);
	struct compat_timeval tv;

	tv.tv_sec = ts.tv_sec;
	tv.tv_usec = ts.tv_nsec / NSEC_PER_USEC;
	return tv;
}
#endif

static inline void scsc_compat_bitmap_copy_le(unsigned long *dst, const void *src, unsigned int nbits)
{
	bitmap_from_arr32(dst, src, nbits);
}
#define bitmap_copy_le scsc_compat_bitmap_copy_le

#ifndef SMC_CMD_CONN_IF
#define SMC_CMD_CONN_IF (0x82000710)
#endif
static inline unsigned long exynos_smc(unsigned long cmd, unsigned long arg0,
				       unsigned long arg1, unsigned long arg2)
{
	struct arm_smccc_res res;

	arm_smccc_smc(cmd, arg0, arg1, arg2, 0, 0, 0, 0, &res);
	return res.a0;
}

#define mm_segment_t int
#define get_fs() 0
#define set_fs(x) ((void)(x))
#define KERNEL_DS 0

#define usermodehelper_read_lock_wait(timeout) 0
#define usermodehelper_read_unlock() do {} while (0)

#include <linux/timer.h>
#include <linux/netdevice.h>
#ifndef from_timer
#define from_timer(var, callback_timer, timer_fieldname) \
	timer_container_of(var, callback_timer, timer_fieldname)
#endif
#ifndef del_timer_sync
#define del_timer_sync timer_delete_sync
#endif
#ifndef netif_rx_ni
#define netif_rx_ni netif_rx
#endif
#define timespec timespec64
#define ktime_to_timespec ktime_to_timespec64
#include <linux/if_arp.h>
#include <linux/hardirq.h>
#ifndef in_irq
#define in_irq in_hardirq
#endif
#ifndef del_timer
#define del_timer timer_delete
#endif

#endif
