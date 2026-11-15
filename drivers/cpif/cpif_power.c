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
#include <linux/delay.h>
#include <linux/device.h>
#include <linux/io.h>
#include <linux/jiffies.h>
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

static int cpif_seq_run(struct cpif_dev *c, const struct cpif_seq_step *seq,
			int n, const char *name)
{
	int i, ret;

	for (i = 0; i < n; i++) {
		const struct cpif_seq_step *s = &seq[i];
		u32 val;
		unsigned long timeout;

		switch (s->op) {
		case CPIF_OP_DELAY:
			msleep(s->value);
			break;
		case CPIF_OP_WRITE:
			if (s->vgpio) {
				val = readl(c->vgpio + s->offset);
				val = (val & ~s->mask) | (s->value & s->mask);
				writel(val, c->vgpio + s->offset);
			} else {
				ret = regmap_update_bits(c->pmu, s->offset,
							 s->mask, s->value & s->mask);
				if (ret)
					goto err;
			}
			break;
		case CPIF_OP_WAIT:
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
	}

	return 0;

err:
	dev_err(c->dev, "%s: step %d failed (%d)\n", name, i, ret);
	return ret;
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

	c->phone_state = CPIF_STATE_ONLINE;
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

	if (mode == CPIF_BOOT_MODE_DUMP) {
		/* cal_cp_enable_dump_pc_no_pg(): CP_OUT bit16 set */
		ret = regmap_update_bits(c->pmu, CP_OUT, 1 << 16, 1 << 16);
		if (ret)
			return ret;
	}

	/* cal_cp_disable_dump_pc_no_pg() first (vendor power_reset path) */
	ret = regmap_update_bits(c->pmu, CP_OUT, 1 << 16, 0);
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

	/* TODO: mbox_set_interrupt()/mbox_sw_reset() once mailbox driver
	 * lands; for now CP_IN wait in release seq is the only sync. */

	if (mode == CPIF_BOOT_MODE_DUMP)
		regmap_update_bits(c->pmu, CP_OUT, 1 << 16, 0);

	return 0;
}
