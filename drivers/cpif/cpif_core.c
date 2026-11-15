// SPDX-License-Identifier: GPL-2.0
/*
 * Core platform driver for the A21s cpif skeleton.
 *
 * Matches "samsung,exynos-cp" (vendor cpif node). Probe responsibilities:
 *  - map PMU syscon regmap (CP power sequences)
 *  - ioremap VGPIO_TX_MONITOR block + ap/cp config regs
 *  - resolve memory-region (cp_rmem @ 0xd0000000/0x8800000)
 *  - parse cp_shmem/regions and ioremap each named region
 *  - register /dev/umts_boot0, /dev/umts_ipc0/1, /dev/umts_rfs0
 *
 * TODO (big-ticket items, see docs/a21s-modem-power-findings.md):
 *  - cp_mailbox driver (0x11920000) for doorbell IRQs + mcu_ipc
 *  - SIPC v5.0 link_device ring-buffer engine (cpif_ipc.c is stub)
 *  - CP_BOOT_MODE security: SMC auth path in cpif_boot.c
 *  - netdev backend for rmnet0-7 (SIOCIF/link_device net path)
 *  - crash/dump support (cp_debug, modem_dump)
 */
#include <linux/io.h>
#include <linux/mfd/syscon.h>
#include <linux/module.h>
#include <linux/miscdevice.h>
#include <linux/of.h>
#include <linux/of_address.h>
#include <linux/platform_device.h>
#include <linux/regmap.h>
#include <linux/slab.h>

#include "cpif.h"

static struct cpif_dev *g_cpif;

struct cpif_dev *cpif_dev_get(void)
{
	return g_cpif;
}
EXPORT_SYMBOL_GPL(cpif_dev_get);

static int cpif_get_pmu(struct cpif_dev *c)
{
	struct device_node *np;

	np = of_parse_phandle(c->dev->of_node, "pmu", 0);
	if (!np) {
		dev_err(c->dev, "missing 'pmu' syscon phandle\n");
		return -ENODEV;
	}
	c->pmu = syscon_node_to_regmap(np);
	of_node_put(np);
	if (IS_ERR(c->pmu)) {
		dev_err(c->dev, "pmu syscon regmap failed: %ld\n",
			PTR_ERR(c->pmu));
		return PTR_ERR(c->pmu);
	}
	return 0;
}

static int cpif_map_regions(struct cpif_dev *c)
{
	struct resource res;
	struct device_node *np, *rmem_np;
	int ret;

	/* cp_rmem: reserved-memory child referenced by memory-region */
	rmem_np = of_parse_phandle(c->dev->of_node, "memory-region", 0);
	if (!rmem_np) {
		dev_err(c->dev, "missing memory-region (cp_rmem)\n");
		return -ENODEV;
	}
	ret = of_address_to_resource(rmem_np, 0, &res);
	of_node_put(rmem_np);
	if (ret) {
		dev_err(c->dev, "cp_rmem resource failed: %d\n", ret);
		return ret;
	}
	c->cp_base = res.start;
	c->cp_size = resource_size(&res);
	dev_info(c->dev, "cp_rmem: phys 0x%llx size 0x%zx\n",
		 (u64)c->cp_base, c->cp_size);

	/* ap2cp_cfg + vgpio monitor from node's own reg entries */
	{
		struct platform_device *pdev = to_platform_device(c->dev);
		struct resource *r;

		r = platform_get_resource(pdev, IORESOURCE_MEM, 0);
		if (r) {
			c->ap2cp_cfg = devm_ioremap_resource(c->dev, r);
			if (IS_ERR(c->ap2cp_cfg))
				return PTR_ERR(c->ap2cp_cfg);
		}
		r = platform_get_resource(pdev, IORESOURCE_MEM, 1);
		if (r) {
			void __iomem *v = devm_ioremap_resource(c->dev, r);
			if (IS_ERR(v)) {
				dev_warn(c->dev,
					 "vgpio-monitor ioremap failed (%ld); continuing\n",
					 PTR_ERR(v));
				c->vgpio = NULL;
			} else {
				c->vgpio = v + VGPIO_TX_MONITOR_OFF;
			}
		} else {
			dev_warn(c->dev,
				 "no vgpio-monitor reg; CP init waits may fail\n");
			c->vgpio = NULL;
		}
	}

	/* cp-mailbox child: keep phandle for future mailbox driver */
	np = of_find_node_by_name(c->dev->of_node, "cp-mailbox");
	if (!np)
		np = of_find_node_by_name(c->dev->of_node, "cp_mailbox");
	if (np)
		of_node_put(np);
	else
		dev_warn(c->dev, "no cp-mailbox child node\n");

	return 0;
}

static const char *cpif_state_name(enum cpif_modem_state st)
{
	switch (st) {
	case CPIF_STATE_OFFLINE:	return "OFFLINE";
	case CPIF_STATE_ONLINE:		return "ONLINE";
	case CPIF_STATE_NV_REBUILDING:	return "NV_REBUILDING";
	case CPIF_STATE_CRASH_EXIT:	return "CRASH_EXIT";
	case CPIF_STATE_SIM_ATTACH:	return "SIM_ATTACH";
	case CPIF_STATE_SIM_DETACH:	return "SIM_DETACH";
	case CPIF_STATE_DUMP_ACTIVE:	return "DUMP_ACTIVE";
	case CPIF_STATE_URC_PENDING:	return "URC_PENDING";
	default:			return "UNKNOWN";
	}
}

static ssize_t modem_name_show(struct device *dev,
			       struct device_attribute *attr, char *buf)
{
	const char *name = "s318ap";

	of_property_read_string(dev->of_node, "mif,name", &name);
	return sysfs_emit(buf, "%s\n", name);
}
static DEVICE_ATTR_RO(modem_name);

static ssize_t phone_state_show(struct device *dev,
				struct device_attribute *attr, char *buf)
{
	struct cpif_dev *c = dev_get_drvdata(dev);

	return sysfs_emit(buf, "%s\n", cpif_state_name(c->phone_state));
}

static ssize_t phone_state_store(struct device *dev,
				 struct device_attribute *attr,
				 const char *buf, size_t count)
{
	struct cpif_dev *c = dev_get_drvdata(dev);
	unsigned int val;

	if (sysfs_streq(buf, "ONLINE"))
		val = CPIF_STATE_ONLINE;
	else if (sysfs_streq(buf, "OFFLINE"))
		val = CPIF_STATE_OFFLINE;
	else if (kstrtouint(buf, 0, &val) || val >= CPIF_STATE_MAX)
		return -EINVAL;

	mutex_lock(&c->lock);
	c->phone_state = (enum cpif_modem_state)val;
	cpif_dump_toc(c);
	wake_up_interruptible(&c->wq);
	mutex_unlock(&c->lock);
	return count;
}
static DEVICE_ATTR_RW(phone_state);

static ssize_t rmem_info_show(struct device *dev,
			      struct device_attribute *attr, char *buf)
{
	struct cpif_dev *c = dev_get_drvdata(dev);

	return sysfs_emit(buf, "0x%llx 0x%zx\n", (u64)c->cp_base, c->cp_size);
}
static DEVICE_ATTR_RO(rmem_info);

static ssize_t toc_summary_show(struct device *dev,
				struct device_attribute *attr, char *buf)
{
	struct cpif_dev *c = dev_get_drvdata(dev);

	return sysfs_emit(buf, "%s\n",
			  c->toc_summary[0] ? c->toc_summary : "unloaded");
}
static DEVICE_ATTR_RO(toc_summary);

static ssize_t mem_map_on_cp_show(struct device *dev,
				  struct device_attribute *attr, char *buf)
{
	struct cpif_dev *c = dev_get_drvdata(dev);

	return sysfs_emit(buf, "%d\n", c->mem_map_on_cp ? 1 : 0);
}
static DEVICE_ATTR_RO(mem_map_on_cp);

static ssize_t regions_summary_show(struct device *dev,
				    struct device_attribute *attr, char *buf)
{
	struct cpif_dev *c = dev_get_drvdata(dev);
	int i, pos = 0;

	for (i = 0; i < CPIF_SHMEM_MAX; i++) {
		struct cpif_shmem_region *r = &c->shmem[i];

		if (!r->map || !r->size)
			continue;
		pos += sysfs_emit_at(buf, pos, "%s%s@0x%llx:0x%zx",
				     pos ? "," : "",
				     r->name ? r->name : "REG",
				     (u64)r->phys, r->size);
	}
	pos += sysfs_emit_at(buf, pos, "\n");
	return pos;
}
static DEVICE_ATTR_RO(regions_summary);

static struct attribute *cpif_attrs[] = {
	&dev_attr_modem_name.attr,
	&dev_attr_phone_state.attr,
	&dev_attr_rmem_info.attr,
	&dev_attr_toc_summary.attr,
	&dev_attr_mem_map_on_cp.attr,
	&dev_attr_regions_summary.attr,
	NULL,
};
ATTRIBUTE_GROUPS(cpif);

static int cpif_probe(struct platform_device *pdev)
{
	struct cpif_dev *c;
	int ret;

	c = devm_kzalloc(&pdev->dev, sizeof(*c), GFP_KERNEL);
	if (!c)
		return -ENOMEM;

	c->dev = &pdev->dev;
	mutex_init(&c->lock);
	init_waitqueue_head(&c->wq);
	c->phone_state = CPIF_STATE_OFFLINE;

	platform_set_drvdata(pdev, c);

	ret = cpif_get_pmu(c);
	if (ret)
		return ret;

	ret = cpif_map_regions(c);
	if (ret)
		return ret;

	ret = cpif_shmem_init(c);
	if (ret)
		return ret;

	ret = cpif_boot_register(c);
	if (ret)
		goto err_shmem;

	ret = cpif_ipc_register(c);
	if (ret)
		goto err_boot;

	g_cpif = c;
	dev_info(c->dev, "a21s-cpif skeleton probed (s318ap, SIPC 5.0)\n");
	cpif_dump_toc(c);
	return 0;

err_boot:
	cpif_boot_unregister(c);
err_shmem:
	cpif_shmem_exit(c);
	return ret;
}

static void cpif_remove(struct platform_device *pdev)
{
	struct cpif_dev *c = platform_get_drvdata(pdev);

	cpif_ipc_unregister(c);
	cpif_boot_unregister(c);
	cpif_shmem_exit(c);
	g_cpif = NULL;
}

static const struct of_device_id cpif_of_match[] = {
	{ .compatible = "samsung,exynos-cp" },
	{ }
};
MODULE_DEVICE_TABLE(of, cpif_of_match);

static struct platform_driver cpif_driver = {
	.probe		= cpif_probe,
	.remove		= cpif_remove,
	.driver		= {
		.name	= "a21s-cpif",
		.of_match_table = cpif_of_match,
		.dev_groups = cpif_groups,
	},
};
module_platform_driver(cpif_driver);

MODULE_DESCRIPTION("A21s Exynos850 Shannon CPIF skeleton (s318ap/SIPC5)");
MODULE_LICENSE("GPL");
