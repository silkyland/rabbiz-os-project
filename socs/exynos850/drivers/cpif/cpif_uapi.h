/* SPDX-License-Identifier: GPL-2.0 */
/*
 * UAPI for Samsung Shannon CPIF modem devices (/dev/umts_*).
 *
 * Definitions mirror vendor drivers/soc/samsung/cpif/modem_prj.h
 * (samsung,exynos-cp, SIPC v5.0, s318ap on Exynos 850 / Galaxy A21s).
 * Keep ioctl numbers byte-identical so vendor userspace (cbd, RIL,
 * libsamsung-ipc) can talk to this driver.
 */
#ifndef __CPIF_UAPI_H__
#define __CPIF_UAPI_H__

#include <linux/ioctl.h>
#include <linux/types.h>

#define CPIF_IOCTL_MAGIC	'o'

#define CPIF_IOC_POWER_ON		_IO(CPIF_IOCTL_MAGIC, 0x19)
#define CPIF_IOC_POWER_OFF		_IO(CPIF_IOCTL_MAGIC, 0x20)

#define CPIF_BOOT_MODE_NORMAL		0
#define CPIF_BOOT_MODE_DUMP		1
#define CPIF_BOOT_MODE_MANUAL		7

struct cpif_boot_mode {
	__u32 idx;
};

#define CPIF_IOC_POWER_RESET		_IOW(CPIF_IOCTL_MAGIC, 0x21, struct cpif_boot_mode)
#define CPIF_IOC_START_CP_BOOTLOADER	_IOW(CPIF_IOCTL_MAGIC, 0x22, struct cpif_boot_mode)
#define CPIF_IOC_COMPLETE_NORMAL_BOOTUP	_IO(CPIF_IOCTL_MAGIC, 0x23)
#define CPIF_IOC_GET_CP_STATUS		_IO(CPIF_IOCTL_MAGIC, 0x27)
#define CPIF_IOC_SET_CP_STATUS		_IOW(CPIF_IOCTL_MAGIC, 0x28, __u32)
#define CPIF_IOC_START_CP_DUMP		_IO(CPIF_IOCTL_MAGIC, 0x32)
#define CPIF_IOC_TRIGGER_CP_CRASH	_IO(CPIF_IOCTL_MAGIC, 0x34)
#define CPIF_IOC_TRIGGER_KERNEL_PANIC	_IO(CPIF_IOCTL_MAGIC, 0x35)

/* Userspace -> kernel firmware upload. */
struct cpif_image {
	__u64 binary;		/* userspace pointer to image bytes */
	__u32 size;		/* total size of image data */
	__u32 m_offset;		/* offset inside CP region (main binary) */
	__u32 b_offset;		/* offset inside bootloader area */
	__u32 mode;		/* 0 = boot area, non-0 = dump area */
	__u32 len;		/* bytes to copy at this call */
} __attribute__((packed));

#define CPIF_IOC_LOAD_CP_IMAGE		_IOW(CPIF_IOCTL_MAGIC, 0x40, struct cpif_image)

/* Secure-boot request. mode/param2/param3 semantics:
 *  mode    : CP_BOOT_MODE_* | (mem_map_on_cp ? cp_shmem_base : 0)
 *  param2  : bootloader size for NORMAL/DUMP, 0 for RE_INIT,
 *            cp_base + bl_size for MANUAL
 *  param3  : main binary size for NORMAL/MANUAL,
 *            cp_base (or cp_ipc base) for DUMP, 0 for RE_INIT
 */
struct cpif_sec_req {
	__u32 mode;
	__u32 param2;
	__u32 param3;
	__u32 param4;
} __attribute__((packed));

#define CPIF_IOC_REQ_SECURITY		_IOW(CPIF_IOCTL_MAGIC, 0x53, struct cpif_sec_req)

/* modem phone_state values returned by CPIF_IOC_GET_CP_STATUS;
 * identical to vendor enum modem_state in modem_prj.h. */
enum cpif_modem_state {
	CPIF_STATE_OFFLINE,
	CPIF_STATE_CRASH_RESET,
	CPIF_STATE_CRASH_EXIT,
	CPIF_STATE_BOOTING,
	CPIF_STATE_ONLINE,
	CPIF_STATE_NV_REBUILDING,
	CPIF_STATE_LOADER_DONE,
	CPIF_STATE_SIM_ATTACH,
	CPIF_STATE_SIM_DETACH,
	CPIF_STATE_CRASH_WATCHDOG,
	CPIF_STATE_MAX
};

/* Log dump indices (CPIF_IOC_GET_LOG_DUMP, not yet implemented). */
#define CPIF_LOG_DUMP_MAX	8
struct cpif_log_dump {
	char name[32];
	__u32 idx;
	__u32 size;
} __attribute__((packed));

#endif /* __CPIF_UAPI_H__ */
