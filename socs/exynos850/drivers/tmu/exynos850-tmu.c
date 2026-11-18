// SPDX-License-Identifier: GPL-2.0
/*
 * Exynos 850 TMU skeleton driver (samsung,exynos850-tmu).
 *
 * Brings up the three sensor groups sharing one TMU block at
 * 0x10070000 (vendor: LITTLE0 id0, LITTLE1 id1, G3D id2 — see
 * exynos3830.dts). For now this driver only maps registers, enables
 * the APB clock and dumps the raw register window to dmesg so the
 * trim/calibration values can be validated on real hardware.
 *
 * TODO before enabling thermal zones:
 *  - get_temp: convert raw TEMP codes to milli-Celsius using the
 *    DT trim properties (tmu_first_point_trim=25C, second=85C,
 *    efuse_value=0x37 per vendor exynos3830.dts). The upstream
 *    acpm-tmu patch series by A. Klimov has the Exynos850 sensor
 *    group table (CPUCL0/CPUCL1/GPU) and register field layout —
 *    either port that or implement the raw-code conversion here.
 *  - Register of_thermal zones: cpu-thermal (sensor 0/1),
 *    gpu-thermal (sensor 2) with passive/hot/critical trips.
 *  - Decide ACPM vs register-level: Klimov's ACPM path reported
 *    ~10C offset on E850-96; register-level (Majewski v4 series)
 *    is probably more accurate.
 */
#include <linux/clk.h>
#include <linux/io.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/platform_device.h>

#define TMU_REG_DUMP_SIZE	0x100

static int exynos850_tmu_probe(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	void __iomem *base;
	struct clk *clk;
	bool clk_on = false;
	int i;

	base = devm_platform_ioremap_resource(pdev, 0);
	if (IS_ERR(base))
		return PTR_ERR(base);

	clk = devm_clk_get_optional(dev, NULL);
	if (!IS_ERR(clk) && clk) {
		clk_prepare_enable(clk);
		clk_on = true;
	}
	platform_set_drvdata(pdev, (void *)(unsigned long)clk_on);

	dev_info(dev, "exynos850-tmu: raw registers @%pa:\n",
		 &pdev->resource[0].start);
	for (i = 0; i < TMU_REG_DUMP_SIZE; i += 16) {
		dev_info(dev, "  %03x: %08x %08x %08x %08x\n", i,
			 readl(base + i), readl(base + i + 4),
			 readl(base + i + 8), readl(base + i + 12));
	}

	/* TODO: implement get_temp + of_thermal registration once the
	 * trim formula is validated against the vendor values. */
	return 0;
}

static void exynos850_tmu_remove(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	struct clk *clk = devm_clk_get_optional(dev, NULL);

	if (!IS_ERR(clk) && clk &&
	    (unsigned long)platform_get_drvdata(pdev))
		clk_disable_unprepare(clk);
}

static const struct of_device_id exynos850_tmu_of_match[] = {
	{ .compatible = "samsung,exynos850-tmu" },
	{ }
};
MODULE_DEVICE_TABLE(of, exynos850_tmu_of_match);

static struct platform_driver exynos850_tmu_driver = {
	.probe		= exynos850_tmu_probe,
	.remove		= exynos850_tmu_remove,
	.driver		= {
		.name	= "exynos850-tmu",
		.of_match_table = exynos850_tmu_of_match,
	},
};
module_platform_driver(exynos850_tmu_driver);

MODULE_DESCRIPTION("Exynos850 TMU register-dump skeleton");
MODULE_LICENSE("GPL");
