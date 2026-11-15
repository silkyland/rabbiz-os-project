// SPDX-License-Identifier: GPL-2.0+
/*
 * s2mpu12-regulator.c - Regulator driver for Samsung S2MPU12 PMIC
 *
 * Adapted for the A21s bare-metal port. The PMIC is reached through the
 * SEC ACPM regmap (sec_pmic_dev->regmap_pmic), NOT i2c. Register map and
 * per-LDO/BUCK voltage tables are taken from the vendor
 * s2mpu12-regulator.c (jameskdev/android_kernel_samsung_a21s-common).
 *
 * Copyright (c) 2019 Samsung Electronics
 */

#include <linux/err.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/regmap.h>
#include <linux/regulator/driver.h>
#include <linux/regulator/of_regulator.h>
#include <linux/mfd/samsung/core.h>
#include <linux/mfd/samsung/s2mpu12.h>

struct s2mpu12_data {
	struct device *dev;
	struct regmap *regmap;
};

static int s2mpu12_enable_regmap(struct regulator_dev *rdev)
{
	struct s2mpu12_data *info = rdev_get_drvdata(rdev);

	return regmap_update_bits(info->regmap, rdev->desc->enable_reg,
				  rdev->desc->enable_mask, rdev->desc->enable_mask);
}

static int s2mpu12_disable_regmap(struct regulator_dev *rdev)
{
	struct s2mpu12_data *info = rdev_get_drvdata(rdev);

	return regmap_update_bits(info->regmap, rdev->desc->enable_reg,
				  rdev->desc->enable_mask, 0);
}

static int s2mpu12_is_enabled_regmap(struct regulator_dev *rdev)
{
	struct s2mpu12_data *info = rdev_get_drvdata(rdev);
	unsigned int val;
	int ret;

	ret = regmap_read(info->regmap, rdev->desc->enable_reg, &val);
	if (ret)
		return ret;

	return (val & rdev->desc->enable_mask) != 0;
}

static int s2mpu12_get_voltage_sel_regmap(struct regulator_dev *rdev)
{
	struct s2mpu12_data *info = rdev_get_drvdata(rdev);
	unsigned int val;
	int ret;

	ret = regmap_read(info->regmap, rdev->desc->vsel_reg, &val);
	if (ret)
		return ret;

	return val & rdev->desc->vsel_mask;
}

static int s2mpu12_set_voltage_sel_regmap(struct regulator_dev *rdev,
					  unsigned int sel)
{
	struct s2mpu12_data *info = rdev_get_drvdata(rdev);

	return regmap_update_bits(info->regmap, rdev->desc->vsel_reg,
				  rdev->desc->vsel_mask, sel);
}

static const struct regulator_ops s2mpu12_ldo_ops = {
	.list_voltage		= regulator_list_voltage_linear,
	.map_voltage		= regulator_map_voltage_linear,
	.is_enabled		= s2mpu12_is_enabled_regmap,
	.enable			= s2mpu12_enable_regmap,
	.disable		= s2mpu12_disable_regmap,
	.get_voltage_sel	= s2mpu12_get_voltage_sel_regmap,
	.set_voltage_sel	= s2mpu12_set_voltage_sel_regmap,
};

static const struct regulator_ops s2mpu12_buck_ops = {
	.list_voltage		= regulator_list_voltage_linear,
	.map_voltage		= regulator_map_voltage_linear,
	.is_enabled		= s2mpu12_is_enabled_regmap,
	.enable			= s2mpu12_enable_regmap,
	.disable		= s2mpu12_disable_regmap,
	.get_voltage_sel	= s2mpu12_get_voltage_sel_regmap,
	.set_voltage_sel	= s2mpu12_set_voltage_sel_regmap,
};

#define LDO_DESC(_name, _id, min, step, ctrl, et) { \
	.name		= _name, \
	.id		= _id, \
	.ops		= &s2mpu12_ldo_ops, \
	.type		= REGULATOR_VOLTAGE, \
	.owner		= THIS_MODULE, \
	.min_uV		= min, \
	.uV_step	= step, \
	.n_voltages	= S2MPU12_LDO_N_VOLTAGES, \
	.vsel_reg	= ctrl, \
	.vsel_mask	= S2MPU12_LDO_VSEL_MASK, \
	.enable_reg	= ctrl, \
	.enable_mask	= S2MPU12_ENABLE_MASK, \
	.enable_time	= et, \
	.of_match	= of_match_ptr(_name), \
	.regulators_node = of_match_ptr("regulators"), \
}

#define BUCK_DESC(_name, _id, min, step, vsel, en, et) { \
	.name		= _name, \
	.id		= _id, \
	.ops		= &s2mpu12_buck_ops, \
	.type		= REGULATOR_VOLTAGE, \
	.owner		= THIS_MODULE, \
	.min_uV		= min, \
	.uV_step	= step, \
	.n_voltages	= S2MPU12_BUCK_N_VOLTAGES, \
	.vsel_reg	= vsel, \
	.vsel_mask	= S2MPU12_BUCK_VSEL_MASK, \
	.enable_reg	= en, \
	.enable_mask	= S2MPU12_ENABLE_MASK, \
	.enable_time	= et, \
	.of_match	= of_match_ptr(_name), \
	.regulators_node = of_match_ptr("regulators"), \
}

static const struct regulator_desc s2mpu12_regulators[] = {
	LDO_DESC("ldo1",  S2MPU12_LDO1,  S2MPU12_LDO_MIN2, S2MPU12_LDO_STEP2, S2MPU12_PMIC_L1CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo2",  S2MPU12_LDO2,  S2MPU12_LDO_MIN4, S2MPU12_LDO_STEP4, S2MPU12_PMIC_L2CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo3",  S2MPU12_LDO3,  S2MPU12_LDO_MIN2, S2MPU12_LDO_STEP2, S2MPU12_PMIC_L3CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo4",  S2MPU12_LDO4,  S2MPU12_LDO_MIN4, S2MPU12_LDO_STEP4, S2MPU12_PMIC_L4CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo5",  S2MPU12_LDO5,  S2MPU12_LDO_MIN2, S2MPU12_LDO_STEP2, S2MPU12_PMIC_L5CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo6",  S2MPU12_LDO6,  S2MPU12_LDO_MIN1, S2MPU12_LDO_STEP1, S2MPU12_PMIC_L6CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo7",  S2MPU12_LDO7,  S2MPU12_LDO_MIN2, S2MPU12_LDO_STEP2, S2MPU12_PMIC_L7CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo8",  S2MPU12_LDO8,  S2MPU12_LDO_MIN3, S2MPU12_LDO_STEP3, S2MPU12_PMIC_L8CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo9",  S2MPU12_LDO9,  S2MPU12_LDO_MIN3, S2MPU12_LDO_STEP3, S2MPU12_PMIC_L9CTRL,  S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo10", S2MPU12_LDO10, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L10CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo11", S2MPU12_LDO11, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L11CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo23", S2MPU12_LDO23, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L23CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo24", S2MPU12_LDO24, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L24CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo25", S2MPU12_LDO25, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L25CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo26", S2MPU12_LDO26, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L26CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo27", S2MPU12_LDO27, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L27CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo28", S2MPU12_LDO28, S2MPU12_LDO_MIN4, S2MPU12_LDO_STEP4, S2MPU12_PMIC_L28CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo29", S2MPU12_LDO29, S2MPU12_LDO_MIN2, S2MPU12_LDO_STEP2, S2MPU12_PMIC_L29CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo30", S2MPU12_LDO30, S2MPU12_LDO_MIN4, S2MPU12_LDO_STEP4, S2MPU12_PMIC_L30CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo31", S2MPU12_LDO31, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L31CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo32", S2MPU12_LDO32, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L32CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo33", S2MPU12_LDO33, S2MPU12_LDO_MIN4, S2MPU12_LDO_STEP4, S2MPU12_PMIC_L33CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo34", S2MPU12_LDO34, S2MPU12_LDO_MIN5, S2MPU12_LDO_STEP5, S2MPU12_PMIC_L34CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo35", S2MPU12_LDO35, S2MPU12_LDO_MIN4, S2MPU12_LDO_STEP4, S2MPU12_PMIC_L35CTRL, S2MPU12_ENABLE_TIME_LDO),
	LDO_DESC("ldo36", S2MPU12_LDO36, S2MPU12_LDO_MIN2, S2MPU12_LDO_STEP2, S2MPU12_PMIC_L36CTRL, S2MPU12_ENABLE_TIME_LDO),

	BUCK_DESC("buck1", S2MPU12_BUCK1, S2MPU12_BUCK_MIN1, S2MPU12_BUCK_STEP1, S2MPU12_PMIC_B1OUT1, S2MPU12_PMIC_B1CTRL, S2MPU12_ENABLE_TIME_BUCK1),
	BUCK_DESC("buck2", S2MPU12_BUCK2, S2MPU12_BUCK_MIN1, S2MPU12_BUCK_STEP1, S2MPU12_PMIC_B2OUT1, S2MPU12_PMIC_B2CTRL, S2MPU12_ENABLE_TIME_BUCK2),
	BUCK_DESC("buck4", S2MPU12_BUCK4, S2MPU12_BUCK_MIN2, S2MPU12_BUCK_STEP2, S2MPU12_PMIC_B4OUT1, S2MPU12_PMIC_B4CTRL, S2MPU12_ENABLE_TIME_BUCK4),
	BUCK_DESC("buck5", S2MPU12_BUCK5, S2MPU12_BUCK_MIN3, S2MPU12_BUCK_STEP3, S2MPU12_PMIC_B5OUT1, S2MPU12_PMIC_B5CTRL, S2MPU12_ENABLE_TIME_BUCK5),
};

static int s2mpu12_regulator_probe(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	struct sec_pmic_dev *iodev;
	struct s2mpu12_data *info;
	struct regulator_config config = { };
	unsigned int rdev_num = ARRAY_SIZE(s2mpu12_regulators);
	int i, ret;

	iodev = dev_get_drvdata(dev->parent);
	if (!iodev || !iodev->regmap_pmic)
		return -ENODEV;

	info = devm_kzalloc(dev, sizeof(*info), GFP_KERNEL);
	if (!info)
		return -ENOMEM;

	info->dev = dev;
	info->regmap = iodev->regmap_pmic;

	platform_set_drvdata(pdev, info);

	if (!dev->of_node)
		device_set_of_node_from_dev(dev, dev->parent);

	config.dev = dev;
	config.driver_data = info;

	for (i = 0; i < rdev_num; i++) {
		struct regulator_dev *rdev;

		rdev = devm_regulator_register(dev, &s2mpu12_regulators[i],
					       &config);
		if (IS_ERR(rdev)) {
			ret = PTR_ERR(rdev);
			dev_err(dev, "failed to register %s: %d\n",
				s2mpu12_regulators[i].name, ret);
			return ret;
		}
	}

	dev_info(dev, "s2mpu12 regulator: %u rails registered\n", rdev_num);
	return 0;
}

static const struct platform_device_id s2mpu12_regulator_id[] = {
	{ .name = "s2mpu12-regulator" },
	{ }
};
MODULE_DEVICE_TABLE(platform, s2mpu12_regulator_id);

static struct platform_driver s2mpu12_regulator_driver = {
	.driver = {
		.name = "s2mpu12-regulator",
	},
	.probe = s2mpu12_regulator_probe,
	.id_table = s2mpu12_regulator_id,
};
module_platform_driver(s2mpu12_regulator_driver);

MODULE_DESCRIPTION("SAMSUNG S2MPU12 Regulator Driver");
MODULE_LICENSE("GPL");
