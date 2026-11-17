// SPDX-License-Identifier: GPL-2.0
/*
 * cp_mailbox (vendor mcu_ipc.c) for the Exynos 850 Shannon modem.
 *
 * ISSR0..3 carry ap2cp_msg / cp2ap_msg / ap2cp_united_status /
 * cp2ap_united_status (MAILBOX_SR in the vendor DT). AP rings CP through
 * INTGR1, CP rings AP through INTSR0[31:16] (one SPI shared by 16 ints).
 */
#include <linux/delay.h>
#include <linux/interrupt.h>
#include <linux/io.h>
#include <linux/of.h>
#include <linux/of_address.h>
#include <linux/of_irq.h>

#include "cpif.h"

#define MBX_MCUCTLR	0x00
#define MBX_INTCR0	0x0c
#define MBX_INTMR0	0x10
#define MBX_INTSR0	0x14
#define MBX_INTGR1	0x1c
#define MBX_INTCR1	0x20
#define MBX_ISSR0	0x80

#define MBX_MSWRST	BIT(0)

/* cp2ap interrupts the vendor DT unmasks: msg, status, active, wakelock, ratmode */
#define CPIF_UNMASKED_IRQS	(BIT(0) | BIT(2) | BIT(3) | BIT(8) | BIT(9))

#define CMD_VALID		0x0040
#define CMD_FIELD		0x003f
#define CMD_INIT_START		0x0001
#define CMD_INIT_END		0x0002
#define CMD_PHONE_START		0x0008
#define CMD_PIF_INIT_DONE	0x000d

u32 cpif_mbox_get(struct cpif_dev *c, u32 mbx)
{
	return readl(c->mbox + MBX_ISSR0 + 4 * mbx);
}

void cpif_mbox_set(struct cpif_dev *c, u32 mbx, u32 val)
{
	writel(val, c->mbox + MBX_ISSR0 + 4 * mbx);
}

u32 cpif_mbox_extract(struct cpif_dev *c, u32 mbx, u32 mask, u32 pos)
{
	return (cpif_mbox_get(c, mbx) >> pos) & mask;
}

void cpif_mbox_update(struct cpif_dev *c, u32 mbx, u32 val, u32 mask, u32 pos)
{
	u32 v = cpif_mbox_get(c, mbx);

	v &= ~(mask << pos);
	v |= (val & mask) << pos;
	cpif_mbox_set(c, mbx, v);
}

void cpif_mbox_raise(struct cpif_dev *c, u32 int_num)
{
	if (int_num < 16)
		writel(BIT(int_num), c->mbox + MBX_INTGR1);
}

void cpif_mbox_sw_reset(struct cpif_dev *c)
{
	writel(readl(c->mbox + MBX_MCUCTLR) | MBX_MSWRST, c->mbox + MBX_MCUCTLR);
	udelay(5);
	writel((u32)(~CPIF_UNMASKED_IRQS) << 16, c->mbox + MBX_INTMR0);
}

void cpif_mbox_dump(struct cpif_dev *c, const char *tag)
{
	dev_info(c->dev, "mbox[%s] ISSR0-3: %08x %08x %08x %08x intmr0 %08x intsr0 %08x\n",
		 tag, cpif_mbox_get(c, 0), cpif_mbox_get(c, 1),
		 cpif_mbox_get(c, 2), cpif_mbox_get(c, 3),
		 readl(c->mbox + MBX_INTMR0), readl(c->mbox + MBX_INTSR0));
}

static irqreturn_t cpif_mbox_isr(int irq, void *data)
{
	struct cpif_dev *c = data;
	u32 stat, i;

	stat = readl(c->mbox + MBX_INTSR0) & 0xffff0000;
	stat &= ~readl(c->mbox + MBX_INTMR0) & 0xffff0000;
	if (!stat)
		return IRQ_NONE;
	writel(stat, c->mbox + MBX_INTCR0);

	for (i = 0; i < 16; i++) {
		if (!(stat & BIT(i + 16)))
			continue;

		c->cp2ap_events |= BIT(i);
		if (i == 0) {
			u32 msg = cpif_mbox_get(c, CPIF_MBX_CP2AP_MSG);

			c->cp2ap_last_cmd = msg;
			dev_info_ratelimited(c->dev, "cp2ap msg 0x%08x\n", msg);
			if ((msg & 0xc0) == 0xc0) {
				int ret;

				mutex_lock(&c->lock);
				switch (msg & CMD_FIELD) {
				case CMD_INIT_START:
					ret = cpif_sbd_bootstrap(c);
					if (ret) {
						dev_err(c->dev, "SBD bootstrap failed: %d\n", ret);
						break;
					}
					cpif_mbox_set(c, CPIF_MBX_AP2CP_MSG,
						      0xc0 | CMD_PIF_INIT_DONE);
					cpif_mbox_raise(c, CPIF_INT_AP2CP_MSG);
					dev_info(c->dev, "PIF_INIT_DONE -> CP\n");
					break;
				case CMD_PHONE_START:
					if (!c->sbd_ready)
						break;
					if (!c->phone_started) {
						ret = cpif_legacy_start_ipc(c);
						if (ret) {
							dev_err(c->dev, "IPC init failed: %d\n", ret);
							break;
						}
						c->phone_started = true;
						complete_all(&c->init_cmpl);
						dev_info(c->dev, "PHONE_START received; IPC ready (RIL not attached)\n");
					}
					/* Vendor: CP repeats PHONE_START until it gets
					 * INIT_END, which is only sent once the FMT
					 * and RFS io_devices are opened (rild_ready).
					 */
					if (cpif_rild_ready(c)) {
						cpif_mbox_set(c, CPIF_MBX_AP2CP_MSG,
							      0xc0 | CMD_INIT_END);
						cpif_mbox_raise(c, CPIF_INT_AP2CP_MSG);
						dev_info(c->dev, "INIT_END -> CP\n");
					}
					break;
				}
				mutex_unlock(&c->lock);
			}
		} else {
			dev_info_ratelimited(c->dev, "cp2ap irq %u (united 0x%08x)\n",
					     i, cpif_mbox_get(c, CPIF_MBX_CP2AP_UNITED));
		}
	}
	wake_up_interruptible(&c->wq);
	return IRQ_HANDLED;
}

int cpif_mbox_init(struct cpif_dev *c)
{
	struct device_node *np;
	u32 reg[2];
	int ret;

	init_completion(&c->init_cmpl);

	np = of_get_child_by_name(c->dev->of_node, "cp-mailbox");
	if (!np)
		np = of_get_child_by_name(c->dev->of_node, "cp_mailbox");
	if (!np) {
		dev_warn(c->dev, "no cp-mailbox child node\n");
		return 0;
	}

	/* the cpif node has no ranges, so read the child's reg raw */
	if (!of_property_read_u32_array(np, "reg", reg, 2))
		c->mbox = ioremap(reg[0], reg[1]);
	c->mbox_irq = irq_of_parse_and_map(np, 0);
	of_node_put(np);
	if (!c->mbox || c->mbox_irq <= 0) {
		dev_err(c->dev, "cp-mailbox map/irq failed\n");
		return -ENODEV;
	}

	writel(0xffff0000, c->mbox + MBX_INTMR0);
	ret = request_threaded_irq(c->mbox_irq, NULL, cpif_mbox_isr,
				   IRQF_NO_SUSPEND | IRQF_ONESHOT, "cpif-mbox", c);
	if (ret) {
		dev_err(c->dev, "mbox irq %d: %d\n", c->mbox_irq, ret);
		iounmap(c->mbox);
		c->mbox = NULL;
		return ret;
	}
	cpif_mbox_dump(c, "init");
	/* POWER_ON's first-boot path does not call power_reset/sw_reset. */
	writel((u32)(~CPIF_UNMASKED_IRQS) << 16, c->mbox + MBX_INTMR0);
	return 0;
}

void cpif_mbox_exit(struct cpif_dev *c)
{
	if (!c->mbox)
		return;
	writel(0xffff0000, c->mbox + MBX_INTMR0);
	free_irq(c->mbox_irq, c);
	iounmap(c->mbox);
	c->mbox = NULL;
}
