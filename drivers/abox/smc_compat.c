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
/* iovmm_set_fault_handler: stubbed via vendor header when
 * CONFIG_EXYNOS_IOMMU is unset (no-op for bring-up). */
