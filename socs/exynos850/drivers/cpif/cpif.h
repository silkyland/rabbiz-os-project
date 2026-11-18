/* SPDX-License-Identifier: GPL-2.0 */
/*
 * Internal header for the a21s (Exynos 850 / Shannon s318ap) CPIF
 * skeleton driver.
 *
 * Based on vendor drivers/soc/samsung/cpif (Linux 4.19.325,
 * samsungexynos850/android_kernel_samsung_exynos850). This driver is
 * intentionally minimal: it implements CP power sequencing (pmucal
 * equivalents as plain MMIO), shared-memory TOC parsing, the
 * /dev/umts_boot0 firmware-upload interface and stubs for the SIPC
 * data channels. It is NOT a drop-in cpif replacement yet.
 */
#ifndef __CPIF_H__
#define __CPIF_H__

#include <linux/miscdevice.h>
#include <linux/types.h>
#include "cpif_uapi.h"

/* Physical addresses specific to Exynos 850 / A21s -------------------- */
#define CPIF_PMU_BASE		0x11860000	/* pmu_system_controller syscon */
#define CPIF_PMU_SIZE		0x10000
#define CPIF_VGPIO_BASE		0x11a50000	/* acpm_mfd_bus / VGPIO_TX_MONITOR */
#define CPIF_VGPIO_SIZE		0x2000
#define CPIF_AP2CP_CFG		0x14020000	/* ap/cp config regs */
#define CPIF_CP_MAILBOX		0x11920000
#define CPIF_CP_MAILBOX_SZ	0x180

/* VGPIO_TX_MONITOR block inside the vgpio-monitor reg region */
#define VGPIO_TX_MONITOR_OFF	0x1700
#define VGPIO_TX_MONITOR_BIT	(1u << 29)

/* CP shmem region names as used in the vendor DT (cp_shmem/regions). */
enum cpif_shmem_idx {
	CPIF_SHMEM_CP	= 0,   /* "CP"  - firmware image + secure mem map */
	CPIF_SHMEM_VSS	= 1,   /* "VSS" - voice call (abox_vss) shared mem */
	CPIF_SHMEM_L2B	= 2,   /* "L2B" */
	CPIF_SHMEM_IPC	= 3,   /* "IPC" - SIPC channel ring buffers */
	CPIF_SHMEM_VPA	= 4,   /* "VPA" */
	CPIF_SHMEM_BTL	= 5,   /* "GOL"/"BTL" - bootloader <-> AP log */
	CPIF_SHMEM_PKTPROC = 6, /* "PKP" */
	CPIF_SHMEM_MAX
};

struct cpif_shmem_region {
	const char	*name;
	phys_addr_t	phys;	/* absolute DRAM address */
	void __iomem	*map;
	size_t		size;
	bool		cached;
	bool		vmapped;	/* map came from vmap(): needs vunmap */
	bool		ioremapped;	/* map came from ioremap(): needs iounmap */
};

struct cpif_dev {
	struct device		*dev;

	struct regmap		*pmu;	/* CP power-control regs */
	void __iomem		*vgpio;	/* VGPIO_TX_MONITOR wait block */
	void __iomem		*ap2cp_cfg;
	void __iomem		*mbox;		/* cp_mailbox 0x11920000 */
	int			mbox_irq;
	u32			cp2ap_events;	/* bitmask of mailbox ints seen */
	u32			cp2ap_last_cmd;
	bool			phone_started;	/* CP sent PHONE_START */
	bool			sbd_ready;	/* SBD v1 descriptors published */
	atomic_t		fmt_opened;	/* umts_ipc* open count (rild_ready) */
	atomic_t		rfs_opened;	/* umts_rfs0 open count (rild_ready) */
	struct completion	init_cmpl;
	int			ds_detect;	/* 1 = single SIM, 2 = dual SIM */

	phys_addr_t		cp_base;	/* cp_rmem base (0xd0000000) */
	size_t			cp_size;	/* cp_rmem size  (0x8800000) */

	struct cpif_shmem_region	shmem[CPIF_SHMEM_MAX];

	enum cpif_modem_state		phone_state;
	bool				mem_map_on_cp;	/* TOC-described map */
	char				toc_summary[256];
	u32				toc_sections;
	wait_queue_head_t		wq;

	struct miscdevice		boot_dev;	/* /dev/umts_boot0 */
	struct miscdevice		ipc_dev[2];	/* /dev/umts_ipc0/1 */
	struct miscdevice		rfs_dev;	/* /dev/umts_rfs0 */

	struct mutex			lock;
};

/* cpif_core.c */
extern struct cpif_dev *cpif_dev_get(void);

/* cpif_mbox.c - cp_mailbox (mcu_ipc) */
#define CPIF_MBX_AP2CP_MSG	0	/* ISSR0 */
#define CPIF_MEM_IPC_MAGIC	0x000000aa
#define CPIF_MBX_CP2AP_MSG	1	/* ISSR1 */
#define CPIF_MBX_AP2CP_UNITED	2	/* ISSR2 */
#define CPIF_MBX_CP2AP_UNITED	3	/* ISSR3 */
#define CPIF_INT_AP2CP_MSG	0
#define CPIF_INT_AP2CP_WAKEUP	1
#define CPIF_INT_AP2CP_STATUS	2
#define CPIF_INT_AP2CP_ACTIVE	3
int  cpif_mbox_init(struct cpif_dev *c);
void cpif_mbox_exit(struct cpif_dev *c);
u32  cpif_mbox_get(struct cpif_dev *c, u32 mbx);
void cpif_mbox_set(struct cpif_dev *c, u32 mbx, u32 val);
void cpif_mbox_update(struct cpif_dev *c, u32 mbx, u32 val, u32 mask, u32 pos);
u32  cpif_mbox_extract(struct cpif_dev *c, u32 mbx, u32 mask, u32 pos);
void cpif_mbox_raise(struct cpif_dev *c, u32 int_num);
void cpif_mbox_sw_reset(struct cpif_dev *c);
void cpif_mbox_dump(struct cpif_dev *c, const char *tag);

/* cpif_power.c - pmucal-equivalent CP power sequences */
#define CPIF_SMC_ID		0x82000700
unsigned long cpif_smc(unsigned long a0, unsigned long a1,
		       unsigned long a2, unsigned long a3);
int cpif_power_on(struct cpif_dev *c);
int cpif_power_off(struct cpif_dev *c);
int cpif_power_reset(struct cpif_dev *c, u32 mode);
int cpif_cp_status(struct cpif_dev *c);
int cpif_power_step(struct cpif_dev *c, int idx);
int cpif_reg_peek(struct cpif_dev *c, int which, u32 off, u32 *val);
int cpif_lin_peek(struct cpif_dev *c, u32 off, u32 *val);
int cpif_lin_poke(struct cpif_dev *c, u32 off, u32 val);
int cpif_legacy_init_now(struct cpif_dev *c);
int cpif_legacy_start_ipc(struct cpif_dev *c);
int cpif_sbd_bootstrap(struct cpif_dev *c);

/* cpif_shmem.c */
int  cpif_shmem_init(struct cpif_dev *c);
void cpif_shmem_exit(struct cpif_dev *c);
void __iomem *cpif_shmem_map(struct cpif_dev *c, int idx);
void __iomem *cpif_shmem_resolve(struct cpif_dev *c, u32 m_offset, u32 len);
phys_addr_t cpif_shmem_base(struct cpif_dev *c, int idx);
size_t cpif_shmem_size(struct cpif_dev *c, int idx);
int  cpif_dump_toc(struct cpif_dev *c);

/* cpif_boot.c - /dev/umts_boot0 */
int cpif_boot_register(struct cpif_dev *c);
void cpif_boot_unregister(struct cpif_dev *c);

/* cpif_ipc.c - /dev/umts_ipc0, /dev/umts_ipc1, /dev/umts_rfs0 stubs */
int cpif_ipc_register(struct cpif_dev *c);
void cpif_ipc_unregister(struct cpif_dev *c);
bool cpif_rild_ready(struct cpif_dev *c);

#endif /* __CPIF_H__ */
