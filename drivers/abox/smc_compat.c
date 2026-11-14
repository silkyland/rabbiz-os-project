/* drivers/abox/smc_compat.c
 *
 * exynos_smc implementation for mainline (replaces vendor
 * arch/arm64/kernel/exynos-smc.S + drivers/soc/samsung/exynos-smc.c)
 */
#include <linux/types.h>
#include <linux/io.h>
#include <linux/iommu.h>
#include <linux/device.h>
#include <exynos_smc.h>

int __exynos_smc(unsigned long cmd, unsigned long arg1,
		unsigned long arg2, unsigned long arg3)
{
	register unsigned long x0 asm("x0") = cmd;
	register unsigned long x1 asm("x1") = arg1;
	register unsigned long x2 asm("x2") = arg2;
	register unsigned long x3 asm("x3") = arg3;

	asm volatile(
		"dsb	sy\n"
		"smc	#0\n"
		: "+r" (x0), "+r" (x1), "+r" (x2), "+r" (x3)
		:
		: "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11",
		  "x12", "x13", "x14", "x15", "x16", "x17", "x18", "x30",
		  "memory");

	return x0;
}
EXPORT_SYMBOL(__exynos_smc);

int exynos_smc(unsigned long cmd, unsigned long arg1,
		unsigned long arg2, unsigned long arg3)
{
	return __exynos_smc(cmd, arg1, arg2, arg3);
}
EXPORT_SYMBOL(exynos_smc);

int exynos_smc_readsfr(unsigned long addr, unsigned long *val)
{
	register unsigned long x0 asm("x0") = SMC_CMD_REG;
	register unsigned long x1 asm("x1") =
		(addr >> 2) | SMC_REG_CLASS_SFR_R;
	register unsigned long x2 asm("x2") = 0;
	register unsigned long x3 asm("x3") = 0;

	asm volatile(
		"dsb	sy\n"
		"smc	#0\n"
		: "+r" (x0), "+r" (x1), "+r" (x2), "+r" (x3)
		:
		: "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11",
		  "x12", "x13", "x14", "x15", "x16", "x17", "x18", "x30",
		  "memory");

	if (x0 == 0 && val)
		*val = x2;

	return x0;
}
EXPORT_SYMBOL(exynos_smc_readsfr);

/* exynos-pmu compat: vendor uses a regmap over the PMU syscon at
 * 0x11860000. For the out-of-tree ABOX port, ioremap the window and
 * provide the read/write/update helpers directly. */
#define EXYNOS_PMU_PHYS	0x11860000
#define EXYNOS_PMU_SIZE	0x10000

static void __iomem *pmu_base;

static void __iomem *pmu_reg(unsigned int offset)
{
	if (!pmu_base)
		pmu_base = ioremap(EXYNOS_PMU_PHYS, EXYNOS_PMU_SIZE);
	if (!pmu_base)
		return NULL;
	return pmu_base + offset;
}

int exynos_pmu_read(unsigned int offset, unsigned int *val)
{
	void __iomem *reg = pmu_reg(offset);

	if (!reg)
		return -ENOMEM;
	*val = readl(reg);
	return 0;
}
EXPORT_SYMBOL(exynos_pmu_read);

int exynos_pmu_write(unsigned int offset, unsigned int val)
{
	void __iomem *reg = pmu_reg(offset);

	if (!reg)
		return -ENOMEM;
	writel(val, reg);
	return 0;
}
EXPORT_SYMBOL(exynos_pmu_write);

int exynos_pmu_update(unsigned int offset, unsigned int mask,
		     unsigned int val)
{
	void __iomem *reg = pmu_reg(offset);
	u32 old;

	if (!reg)
		return -ENOMEM;
	old = readl(reg);
	writel((old & ~mask) | (val & mask), reg);
	return 0;
}
EXPORT_SYMBOL(exynos_pmu_update);

/* iovmm_set_fault_handler: stubbed via vendor header when
 * CONFIG_EXYNOS_IOMMU is unset (no-op for bring-up). */

/* set_codec_notifier_flag lives in the vendor S2MPU12 MFD driver.
 * Weak stub for bring-up: codec jack-detection IRQ notification is
 * inert without the PMIC MFD stack. */
__weak void set_codec_notifier_flag(bool on)
{
}
EXPORT_SYMBOL(set_codec_notifier_flag);
