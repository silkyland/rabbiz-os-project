// SPDX-License-Identifier: GPL-2.0
/*
 * Read the firmware-initialized Exynos850 TMU without changing its
 * registers. The Exynos3830 vendor DT maps LITTLE0, LITTLE1 and G3D
 * to physical sensors 1, 2 and 3 respectively.
 */
#include <linux/bitfield.h>
#include <linux/clk.h>
#include <linux/hwmon.h>
#include <linux/io.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/thermal.h>

#define TMU_CONTROL		0x20
#define TMU_CORE_ENABLE		BIT(0)
#define TMU_CURRENT_TEMP_1_0	0x40
#define TMU_CURRENT_TEMP_4_2	0x44
#define TMU_CODE_MASK		GENMASK(8, 0)
#define TMU_TRIM_85_MASK		GENMASK(17, 9)
#define TMU_TWO_POINT_TRIM	BIT(23)
#define TMU_TRIM_25_MC		25000
#define TMU_TRIM_INTERVAL_MC	60000

static const struct {
	const char *label;
	unsigned int reg;
	unsigned int shift;
} tmu_channels[] = {
	{ "LITTLE0", TMU_CURRENT_TEMP_1_0, 9 },
	{ "LITTLE1", TMU_CURRENT_TEMP_4_2, 0 },
	{ "G3D", TMU_CURRENT_TEMP_4_2, 9 },
};

struct exynos850_tmu_sensor {
	struct exynos850_tmu *tmu;
	unsigned int channel;
	int code_25;
	int code_85;
};

struct exynos850_tmu {
	void __iomem *base;
	bool two_point;
	struct exynos850_tmu_sensor sensors[ARRAY_SIZE(tmu_channels)];
};

static umode_t exynos850_tmu_visible(const void *data,
				    enum hwmon_sensor_types type,
				    u32 attr, int channel)
{
	return 0444;
}

static int exynos850_tmu_get_temp(struct exynos850_tmu_sensor *sensor, int *temp)
{
	struct exynos850_tmu *tmu = sensor->tmu;
	unsigned int channel = sensor->channel;
	int code, delta;

	if (!(readl(tmu->base + TMU_CONTROL) & TMU_CORE_ENABLE))
		return -ENODATA;

	code = (readl(tmu->base + tmu_channels[channel].reg) >>
		tmu_channels[channel].shift) & TMU_CODE_MASK;
	if (!code)
		return -ENODATA;

	delta = code - sensor->code_25;
	if (tmu->two_point)
		*temp = TMU_TRIM_25_MC + delta * TMU_TRIM_INTERVAL_MC /
			(sensor->code_85 - sensor->code_25);
	else
		*temp = TMU_TRIM_25_MC + delta * 1000;

	return 0;
}

static int exynos850_tmu_read(struct device *dev, enum hwmon_sensor_types type,
			      u32 attr, int channel, long *value)
{
	struct exynos850_tmu *tmu = dev_get_drvdata(dev);
	int temp, ret;

	ret = exynos850_tmu_get_temp(&tmu->sensors[channel], &temp);
	if (!ret)
		*value = temp;
	return ret;
}

static int exynos850_tmu_thermal_read(struct thermal_zone_device *zone, int *temp)
{
	return exynos850_tmu_get_temp(thermal_zone_device_priv(zone), temp);
}

static const struct thermal_zone_device_ops exynos850_tmu_thermal_ops = {
	.get_temp = exynos850_tmu_thermal_read,
};

static int exynos850_tmu_read_string(struct device *dev,
				     enum hwmon_sensor_types type,
				     u32 attr, int channel, const char **value)
{
	*value = tmu_channels[channel].label;
	return 0;
}

static const struct hwmon_ops exynos850_tmu_hwmon_ops = {
	.is_visible = exynos850_tmu_visible,
	.read = exynos850_tmu_read,
	.read_string = exynos850_tmu_read_string,
};

static const struct hwmon_channel_info * const exynos850_tmu_info[] = {
	HWMON_CHANNEL_INFO(temp, HWMON_T_INPUT | HWMON_T_LABEL,
			  HWMON_T_INPUT | HWMON_T_LABEL,
			  HWMON_T_INPUT | HWMON_T_LABEL),
	NULL
};

static const struct hwmon_chip_info exynos850_tmu_chip = {
	.ops = &exynos850_tmu_hwmon_ops,
	.info = exynos850_tmu_info,
};

static int exynos850_tmu_probe(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	struct device *hwmon;
	struct exynos850_tmu *tmu;
	struct resource *res;
	struct clk *clk;
	unsigned int i;
	u32 trim;

	tmu = devm_kzalloc(dev, sizeof(*tmu), GFP_KERNEL);
	if (!tmu)
		return -ENOMEM;

	tmu->base = devm_platform_get_and_ioremap_resource(pdev, 0, &res);
	if (IS_ERR(tmu->base))
		return PTR_ERR(tmu->base);

	if (resource_size(res) < TMU_CURRENT_TEMP_4_2 + sizeof(u32))
		return dev_err_probe(dev, -EINVAL, "TMU register window too small\n");

	clk = devm_clk_get_enabled(dev, NULL);
	if (IS_ERR(clk))
		return dev_err_probe(dev, PTR_ERR(clk), "Failed to enable TMU clock\n");

	tmu->two_point = readl(tmu->base) & TMU_TWO_POINT_TRIM;
	for (i = 0; i < ARRAY_SIZE(tmu_channels); i++) {
		struct exynos850_tmu_sensor *sensor = &tmu->sensors[i];

		trim = readl(tmu->base + (i + 1) * sizeof(u32));
		sensor->tmu = tmu;
		sensor->channel = i;
		sensor->code_25 = FIELD_GET(TMU_CODE_MASK, trim);
		sensor->code_85 = FIELD_GET(TMU_TRIM_85_MASK, trim);
		if (!sensor->code_25 ||
		    (tmu->two_point && sensor->code_85 <= sensor->code_25))
			return dev_err_probe(dev, -EINVAL, "Invalid %s eFuse trim\n",
					     tmu_channels[i].label);

		dev_info(dev, "%s: %s-point trim, code25=%d code85=%d\n",
			 tmu_channels[i].label, tmu->two_point ? "two" : "one",
			 sensor->code_25, sensor->code_85);
	}

	for (i = 0; i < ARRAY_SIZE(tmu_channels); i++) {
		struct thermal_zone_device *zone;

		zone = devm_thermal_of_zone_register(dev, i + 1,
						    &tmu->sensors[i],
						    &exynos850_tmu_thermal_ops);
		if (IS_ERR(zone) && PTR_ERR(zone) != -ENODEV)
			return dev_err_probe(dev, PTR_ERR(zone),
					     "Failed to register %s thermal zone\n",
					     tmu_channels[i].label);
	}

	hwmon = devm_hwmon_device_register_with_info(dev, "exynos850_tmu", tmu,
						   &exynos850_tmu_chip, NULL);
	return PTR_ERR_OR_ZERO(hwmon);
}

static const struct of_device_id exynos850_tmu_of_match[] = {
	{ .compatible = "samsung,exynos850-tmu" },
	{ }
};
MODULE_DEVICE_TABLE(of, exynos850_tmu_of_match);

static struct platform_driver exynos850_tmu_driver = {
	.probe = exynos850_tmu_probe,
	.driver = {
		.name = "exynos850-tmu",
		.of_match_table = exynos850_tmu_of_match,
	},
};
module_platform_driver(exynos850_tmu_driver);

MODULE_DESCRIPTION("Exynos850 firmware-initialized TMU temperature monitor");
MODULE_LICENSE("GPL");
