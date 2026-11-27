// SPDX-License-Identifier: GPL-2.0
/*
 * CP power control for Exynos 850 (Shannon s318ap).
 *
 * Direct translation of the vendor pmucal sequence tables in
 * drivers/soc/samsung/cal-if/exynos3830/pmucal_cp_exynos3830.h
 * (PMUCAL_WRITE / PMUCAL_WAIT / PMUCAL_DELAY rows) onto a plain
 * regmap over the PMU syscon (0x11860000). The single exception is
 * VGPIO_TX_MONITOR which lives outside the PMU block at
 * 0x11a50000+0x1700 (acpm_mfd_bus) and is ioremapped separately.
 *
 * TODO(hardware validation):
 *  - confirm PMU regs 0x3000..0x3b78 are non-secure accessible on
 *    A21s (vendor routes some accesses through pmucal_rae -> SMC).
 *    If regmap_read returns bus errors, add an SMC fallback.
 *  - verify wait timeouts (vendor uses ~1s for VGPIO_TX_MONITOR).
 */
#include <linux/arm-smccc.h>
#include <linux/capability.h>
#include <linux/delay.h>
#include <linux/device.h>
#include <linux/io.h>
#include <linux/jiffies.h>
#include <linux/mm.h>
#include <linux/regmap.h>

#include "cpif.h"

/* PMU register offsets (from pmucal_cp_exynos3830.h) */
#define CP_CTRL_S		0x3014
#define CP_OPTION		0x300c
#define CP_CONFIGURATION	0x3000
#define CP_STATUS		0x3004
#define CP_OUT			0x3020
#define CP_IN			0x3024
#define CP_INT_EN		0x3044
#define MIF_CTRL		0x3810
#define TOP_OUT			0x3920
#define TCXO_BUF_CTRL		0x3b78

/* VGPIO_TX_MONITOR (second reg bank, base 0x11a50000) — offsets in cpif.h */
#define CPIF_WAIT_TIMEOUT_MS	1000
#define CPIF_WAIT_POLL_US	100

/* CP_CTRL_S is secure-only on the A21s: reads fault, so it goes via SMC */
#define CPIF_SMC_READ_CTRL	0x3
#define CPIF_SMC_WRITE_CTRL	0x4
#define CPIF_SMC_CTRL_S		0

unsigned long cpif_smc(unsigned long a0, unsigned long a1,
		       unsigned long a2, unsigned long a3)
{
	struct arm_smccc_res res;

	arm_smccc_smc(a0, a1, a2, a3, 0, 0, 0, 0, &res);
	return res.a0;
}

static int cpif_ctrl_s_update(struct cpif_dev *c, u32 mask, u32 value)
{
	unsigned long lo, hi, ret;
	u32 cur, new;

	lo = cpif_smc(CPIF_SMC_ID, CPIF_SMC_READ_CTRL, 0, CPIF_SMC_CTRL_S);
	hi = cpif_smc(CPIF_SMC_ID, CPIF_SMC_READ_CTRL, 1, CPIF_SMC_CTRL_S);
	if ((lo & 0xffff) || (hi & 0xffff)) {
		dev_err(c->dev, "CP_CTRL_S smc read failed lo 0x%lx hi 0x%lx\n", lo, hi);
		return -EIO;
	}
	cur = (((u32)hi >> 16) << 16) | ((u32)lo >> 16);
	new = (cur & ~mask) | (value & mask);
	ret = cpif_smc(CPIF_SMC_ID, CPIF_SMC_WRITE_CTRL, new, CPIF_SMC_CTRL_S);
	dev_info(c->dev, "CP_CTRL_S 0x%08x -> 0x%08x (smc ret 0x%lx)\n", cur, new, ret);
	return ret ? -EIO : 0;
}

enum cpif_seq_op {
	CPIF_OP_WRITE,		/* (reg & ~mask) | (value & mask) */
	CPIF_OP_WAIT,		/* poll until (reg & mask) == (value & mask) */
	CPIF_OP_DELAY,		/* msleep(value) */
};

struct cpif_seq_step {
	u32		op;
	u32		offset;	/* PMU offset, or VGPIO offset if vgpio=1 */
	u32		mask;
	u32		value;
	u32		vgpio;		/* use vgpio bank instead of PMU */
};

/* cp_init[] from pmucal_cp_exynos3830.h */
static const struct cpif_seq_step cp_init_seq[] = {
	{ CPIF_OP_WRITE, CP_CTRL_S,		1 << 3,	1 << 3,	0 },
	{ CPIF_OP_WRITE, CP_OPTION,		1 << 3,	1 << 3,	0 },
	{ CPIF_OP_DELAY, 0,			0,	3,	0 },
	{ CPIF_OP_WRITE, TOP_OUT,		1 << 1,	1 << 1,	0 },
	{ CPIF_OP_WAIT,	 VGPIO_TX_MONITOR_OFF, VGPIO_TX_MONITOR_BIT,
					VGPIO_TX_MONITOR_BIT,	1 },
	{ CPIF_OP_DELAY, 0,			0,	0x3e8,	0 },
	{ CPIF_OP_WRITE, CP_CONFIGURATION,	1 << 0,	1 << 0,	0 },
	{ CPIF_OP_WAIT,	 CP_STATUS,		1 << 0,	1 << 0,	0 },
	{ CPIF_OP_WAIT,	 CP_IN,		1 << 4,	1 << 4,	0 },
};

/* cp_reset_release[] */
static const struct cpif_seq_step cp_reset_release_seq[] = {
	{ CPIF_OP_WRITE, CP_OPTION,		1 << 3,	1 << 3,	0 },
	{ CPIF_OP_WAIT,	 MIF_CTRL,		1 << 0,	1 << 0,	0 },
	{ CPIF_OP_WRITE, TCXO_BUF_CTRL,	1 << 0,	1 << 0,	0 },
	{ CPIF_OP_WRITE, TOP_OUT,		1 << 1,	1 << 1,	0 },
	{ CPIF_OP_WAIT,	 VGPIO_TX_MONITOR_OFF, VGPIO_TX_MONITOR_BIT,
					VGPIO_TX_MONITOR_BIT,	1 },
	{ CPIF_OP_DELAY, 0,			0,	0x3e8,	0 },
	{ CPIF_OP_WRITE, CP_CONFIGURATION,	1 << 0,	1 << 0,	0 },
	{ CPIF_OP_WAIT,	 CP_STATUS,		1 << 0,	1 << 0,	0 },
	{ CPIF_OP_WRITE, CP_INT_EN,		1 << 3,	1 << 3,	0 },
	{ CPIF_OP_WRITE, CP_INT_EN,		1 << 5,	1 << 5,	0 },
	{ CPIF_OP_WAIT,	 CP_IN,		1 << 4,	1 << 4,	0 },
};

static int cpif_seq_step_run(struct cpif_dev *c, const struct cpif_seq_step *s,
			     int i, const char *name)
{
	u32 val;
	unsigned long timeout;
	int ret = 0;

	dev_info(c->dev, "%s: step %d op %u %s+0x%x mask 0x%x val 0x%x\n",
		 name, i, s->op, s->vgpio ? "vgpio" : "pmu",
		 s->offset, s->mask, s->value);

	switch (s->op) {
	case CPIF_OP_DELAY:
		msleep(s->value);
		break;
	case CPIF_OP_WRITE:
		if (s->vgpio) {
			if (!c->vgpio || s->offset + sizeof(u32) > c->vgpio_size) {
				dev_err(c->dev, "%s: step %d vgpio+0x%x out of range (size 0x%pa)\n",
					name, i, s->offset, &c->vgpio_size);
				ret = -EINVAL;
				goto err;
			}
			val = readl(c->vgpio + s->offset);
			val = (val & ~s->mask) | (s->value & s->mask);
			writel(val, c->vgpio + s->offset);
		} else if (s->offset == CP_CTRL_S) {
			ret = cpif_ctrl_s_update(c, s->mask, s->value);
			if (ret)
				goto err;
		} else {
			ret = regmap_update_bits(c->pmu, s->offset,
						 s->mask, s->value & s->mask);
			if (ret)
				goto err;
		}
		break;
	case CPIF_OP_WAIT:
		if (s->vgpio && (!c->vgpio || s->offset + sizeof(u32) > c->vgpio_size)) {
			dev_err(c->dev, "%s: step %d vgpio+0x%x out of range (size 0x%pa)\n",
				name, i, s->offset, &c->vgpio_size);
			ret = -EINVAL;
			goto err;
		}
		timeout = jiffies + msecs_to_jiffies(CPIF_WAIT_TIMEOUT_MS);
		for (;;) {
			if (s->vgpio)
				val = readl(c->vgpio + s->offset);
			else {
				ret = regmap_read(c->pmu, s->offset, &val);
				if (ret)
					goto err;
			}
			if ((val & s->mask) == (s->value & s->mask))
				break;
			if (time_after(jiffies, timeout)) {
				dev_err(c->dev, "%s: step %d wait "
					"timeout @%s+0x%x mask 0x%x val 0x%x\n",
					name, i, s->vgpio ? "vgpio" : "pmu",
					s->offset, s->mask, val);
				ret = -ETIMEDOUT;
				goto err;
			}
			usleep_range(CPIF_WAIT_POLL_US,
				     CPIF_WAIT_POLL_US * 2);
		}
		break;
	default:
		dev_err(c->dev, "%s: bad op %u\n", name, s->op);
		return -EINVAL;
	}

	dev_info(c->dev, "%s: step %d done\n", name, i);
	return 0;

err:
	dev_err(c->dev, "%s: step %d failed (%d)\n", name, i, ret);
	return ret;
}

static int cpif_seq_run(struct cpif_dev *c, const struct cpif_seq_step *seq,
			int n, const char *name)
{
	int i, ret;

	for (i = 0; i < n; i++) {
		ret = cpif_seq_step_run(c, &seq[i], i, name);
		if (ret)
			return ret;
	}

	return 0;
}

/* Run exactly one cp_init[] step; used by the power_step debug attr to
 * bisect which step wedges the SoC. Index space = cp_init_seq[]. */
int cpif_power_step(struct cpif_dev *c, int idx)
{
	if (idx < 0 || idx >= ARRAY_SIZE(cp_init_seq))
		return -EINVAL;

	return cpif_seq_step_run(c, &cp_init_seq[idx], idx, "cp_init");
}

/* Read-only register peek for bring-up debugging. which: 0 = PMU syscon,
 * 1 = vgpio-monitor bank, 2 = CP_CTRL_S via SMC (read-only halves). */
int cpif_reg_peek(struct cpif_dev *c, int which, u32 off, u32 *val)
{
	unsigned long lo, hi;

	if (!capable(CAP_SYS_ADMIN))
		return -EPERM;

	if (which == 0)
		return regmap_read(c->pmu, off, val);
	if (which == 1) {
		if (!c->vgpio)
			return -ENODEV;
		if ((off & 3) || c->vgpio_size < sizeof(u32) ||
		    off > c->vgpio_size - sizeof(u32))
			return -EINVAL;
		*val = readl(c->vgpio + off);
		return 0;
	}
	if (which == 2) {
		lo = cpif_smc(CPIF_SMC_ID, CPIF_SMC_READ_CTRL, 0,
			      CPIF_SMC_CTRL_S);
		hi = cpif_smc(CPIF_SMC_ID, CPIF_SMC_READ_CTRL, 1,
			      CPIF_SMC_CTRL_S);
		if ((lo & 0xffff) || (hi & 0xffff))
			return -EIO;
		*val = (((u32)hi >> 16) << 16) | ((u32)lo >> 16);
		return 0;
	}
	/* 3=IPC 4=BTL 5=CP 6=VSS shmem regions */
	if (which >= 3 && which <= 6) {
		static const int idx[] = {
			CPIF_SHMEM_IPC, CPIF_SHMEM_BTL,
			CPIF_SHMEM_CP, CPIF_SHMEM_VSS,
		};
		size_t sz = c->shmem[idx[which - 3]].size;
		void __iomem *m = c->shmem[idx[which - 3]].map;

		if (!m || (off & 3) || sz < sizeof(u32) ||
		    off > sz - sizeof(u32))
			return -EINVAL;
		*val = readl(m + off);
		return 0;
	}
	return -EINVAL;
}

/* Debug: access cp_rmem through the kernel linear map (phys_to_virt),
 * the same view vendor shm_ipc uses (phys_to_virt / vmap of pages)
 * rather than ioremap. Used to tell an ioremap artifact apart from a
 * hardware-level write block on the "secure" 0x6900000 window. */
int cpif_lin_peek(struct cpif_dev *c, u32 off, u32 *val)
{
	unsigned long pa;
	void *p;

	if (!capable(CAP_SYS_ADMIN))
		return -EPERM;

	if ((off & 3) || c->cp_size < sizeof(u32) ||
	    off > c->cp_size - sizeof(u32))
		return -EINVAL;
	pa = (unsigned long)(c->cp_base + off);
	if (!pfn_valid(pa >> PAGE_SHIFT))
		return -ENXIO;
	p = phys_to_virt(pa);
	if (!p)
		return -ENXIO;
	*val = readl_relaxed((void __iomem *)p);
	return 0;
}

int cpif_lin_poke(struct cpif_dev *c, u32 off, u32 val)
{
	unsigned long pa;
	void *p;

	if (!capable(CAP_SYS_ADMIN))
		return -EPERM;

	if ((off & 3) || c->cp_size < sizeof(u32) ||
	    off > c->cp_size - sizeof(u32))
		return -EINVAL;
	pa = (unsigned long)(c->cp_base + off);
	if (!pfn_valid(pa >> PAGE_SHIFT))
		return -ENXIO;
	p = phys_to_virt(pa);
	if (!p)
		return -ENXIO;
	writel_relaxed(val, (void __iomem *)p);
	return 0;
}

/* cal_cp_status(): 0 = off, 1 = on */
int cpif_cp_status(struct cpif_dev *c)
{
	u32 val;
	int ret;

	ret = regmap_read(c->pmu, CP_STATUS, &val);
	if (ret)
		return ret;

	return !!(val & (1 << 0));
}

int cpif_power_on(struct cpif_dev *c)
{
	int ret;

	c->phone_state = CPIF_STATE_OFFLINE;

	ret = cpif_cp_status(c);
	if (ret < 0)
		return ret;

	if (ret == 0) {
		/* First-time init sequence (vendor: cal_cp_init) */
		ret = cpif_seq_run(c, cp_init_seq, ARRAY_SIZE(cp_init_seq),
				   "cp_init");
		if (ret)
			return ret;
	}

	return 0;
}

int cpif_power_off(struct cpif_dev *c)
{
	/* cal_cp_reset_assert(): clear CP_CONFIGURATION, wait status low */
	int ret = cpif_seq_run(c, (struct cpif_seq_step[]){
		{ CPIF_OP_WRITE, CP_CONFIGURATION, 1 << 0, 0 << 0, 0 },
		{ CPIF_OP_WAIT,	 CP_STATUS,	     1 << 0, 0 << 0, 0 },
	}, 2, "cp_assert");

	if (!ret)
		c->phone_state = CPIF_STATE_OFFLINE;
	return ret;
}

int cpif_power_reset(struct cpif_dev *c, u32 mode)
{
	int ret;

	/* vendor power_reset_cp(): nothing to reset until the CP was booted */
	if (c->phone_state == CPIF_STATE_OFFLINE)
		return 0;

	c->phone_state = mode == CPIF_BOOT_MODE_DUMP ?
		CPIF_STATE_CRASH_EXIT : CPIF_STATE_OFFLINE;

	if (cpif_cp_status(c) <= 0)
		return 0;

	if (c->mbox) {
		cpif_mbox_raise(c, CPIF_INT_AP2CP_WAKEUP);
		usleep_range(5000, 10000);
	}

	ret = regmap_update_bits(c->pmu, CP_OUT, 1 << 16,
				 mode == CPIF_BOOT_MODE_DUMP ? 1 << 16 : 0);
	if (ret)
		return ret;

	ret = cpif_power_off(c);
	if (ret)
		return ret;

	usleep_range(5000, 10000);
	ret = cpif_seq_run(c, cp_reset_release_seq,
			   ARRAY_SIZE(cp_reset_release_seq), "cp_release");
	if (ret)
		return ret;

	if (c->mbox)
		cpif_mbox_sw_reset(c);

	return 0;
}
