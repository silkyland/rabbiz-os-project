// SPDX-License-Identifier: GPL-2.0
/*
 * Minimal S2MU106 charger bring-up for the Galaxy A21s (Exynos 850)
 * mainline port. Register addresses and formulas are taken from the
 * vendor driver (drivers/battery/charger/s2mu106_charger).
 *
 * The full vendor stack (MFD + MUIC + USB-PD) is not ported yet; this
 * driver does the smallest safe job: enable the charge switch
 * (REG_MODE=CHG), program sane limits, keep the hardware watchdog
 * alive, and expose a power_supply node so upower/DEs can see the
 * charger. USB-C PD/Rd assertion for sources that gate VBUS on CC is
 * out of scope here - dumb 5 V sources (USB-A, wall adapters, host
 * ports that always drive VBUS) already charge.
 */

#include <linux/delay.h>
#include <linux/i2c.h>
#include <linux/minmax.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/power_supply.h>
#include <linux/workqueue.h>

#define S2MU106_CHG_STATUS0	0x0e
#define S2MU106_CHG_STATUS1	0x0f
#define S2MU106_CHG_STATUS3	0x11
#define S2MU106_CHG_CTRL0	0x18
#define S2MU106_CHG_CTRL1	0x19
#define S2MU106_CHG_CTRL5	0x1d
#define S2MU106_CHG_CTRL7	0x1f
#define S2MU106_CHG_CTRL10	0x22
#define S2MU106_CHG_CTRL12	0x24
#define S2MU106_CHG_CTRL13	0x25

#define REG_MODE_MASK		0x0f
#define REG_MODE_CHG		0x03
#define CHGIN_STATUS_MASK	0x07
#define DONE_STATUS_MASK	0x01

#define S2MU106_FACTORY_OFF	0xf3	/* bit1: HW factory off, must be 0 */
#define S2MU106_VSSH_LDO	0x3c	/* bits[5:4]: VSSH LDO (non-factory 0x10) */
#define S2MU106_ICR		0x7d	/* bit1: input-current-regulation off */

#define WDT_CLR			0x01	/* CTRL12 bit0 */
#define WDT_EN			0x10	/* CTRL12 bit4 */
#define WDT_TIME_100S		(0x07 << 1)
#define FC_TIMER_16H		(0x05 << 3)	/* CTRL13 bits[5:3] */

/* Register encodings (vendor formulas) */
#define CV_MV(x)		(((x) - 3900) / 5)		/* CTRL5: float V */
#define INLIM_MA(x)		(((x) - 50) / 25)		/* CTRL1: input mA */
#define FAST_MA(x)		(((x) / 50) - 1)		/* CTRL7: charge mA */
#define TOPOFF_MA(x)		(((x) - 100) / 25)		/* CTRL10 nibble */

#define CTRL1_INLIM_MASK	0x7f
#define CTRL5_CV_MASK		0x7f
#define CTRL7_FAST_MASK		0x3f

/* Sysfs-writable limits: the defaults suit a host port; a wall charger
 * can be given more (userspace picks, there is no MUIC cable detection yet). */
#define INPUT_MA_MIN		100
#define INPUT_MA_MAX		2000
#define CHARGE_MA_MIN		100
#define CHARGE_MA_MAX		2000

#define CV_MV_VAL		4350
#define INPUT_MA_VAL		1500
#define CHARGE_MA_VAL		1400
#define TOPOFF1_MA		150
#define TOPOFF2_MA		100

#define WD_KICK_MS		40000	/* < WDT timeout (100 s) */

struct s2mu106_charger {
	struct i2c_client *client;
	struct power_supply *psy;
	struct delayed_work wdog;
	bool last_online;
};

static int s2mu106_update(struct i2c_client *c, u8 reg, u8 val, u8 mask)
{
	int v = i2c_smbus_read_byte_data(c, reg);

	if (v < 0)
		return v;
	return i2c_smbus_write_byte_data(c, reg, (v & ~mask) | (val & mask));
}

static int s2mu106_set_input_ma(struct i2c_client *c, int ma)
{
	ma = clamp(ma, INPUT_MA_MIN, INPUT_MA_MAX);
	return s2mu106_update(c, S2MU106_CHG_CTRL1, INLIM_MA(ma),
			      CTRL1_INLIM_MASK);
}

static int s2mu106_set_charge_ma(struct i2c_client *c, int ma)
{
	ma = clamp(ma, CHARGE_MA_MIN, CHARGE_MA_MAX);
	return s2mu106_update(c, S2MU106_CHG_CTRL7, FAST_MA(ma),
			      CTRL7_FAST_MASK);
}

static int s2mu106_read_online(struct i2c_client *c)
{
	int v = i2c_smbus_read_byte_data(c, S2MU106_CHG_STATUS0);

	if (v < 0)
		return v;
	v &= CHGIN_STATUS_MASK;
	return v == 0x03 || v == 0x05;
}

static void s2mu106_program(struct s2mu106_charger *chg)
{
	struct i2c_client *c = chg->client;

	/* vendor s2mu106_chg_init essentials */
	s2mu106_update(c, S2MU106_FACTORY_OFF, 0x00, 0x02);
	s2mu106_update(c, S2MU106_VSSH_LDO, 0x10, 0x30);
	s2mu106_update(c, S2MU106_ICR, 0x02, 0x02);
	s2mu106_update(c, S2MU106_CHG_CTRL5, CV_MV(CV_MV_VAL), CTRL5_CV_MASK);
	s2mu106_set_input_ma(c, INPUT_MA_VAL);
	s2mu106_set_charge_ma(c, CHARGE_MA_VAL);
	i2c_smbus_write_byte_data(c, S2MU106_CHG_CTRL10,
		(TOPOFF_MA(TOPOFF1_MA) << 4) | TOPOFF_MA(TOPOFF2_MA));
	s2mu106_update(c, S2MU106_CHG_CTRL13, FC_TIMER_16H, 0x38);
	/* watchdog: 100 s, enabled - the work function kicks it */
	s2mu106_update(c, S2MU106_CHG_CTRL12,
		WDT_TIME_100S | WDT_EN, 0x1e);
	/* charge switch on */
	s2mu106_update(c, S2MU106_CHG_CTRL0, REG_MODE_CHG, REG_MODE_MASK);
}

static void s2mu106_wdog(struct work_struct *work)
{
	struct s2mu106_charger *chg =
		container_of(work, struct s2mu106_charger, wdog.work);
	struct i2c_client *c = chg->client;
	int online = s2mu106_read_online(c);
	int mode;

	s2mu106_update(c, S2MU106_CHG_CTRL12, WDT_CLR, WDT_CLR);

	/* re-assert charge mode if the chip lost it (POR, fault, replug) */
	mode = i2c_smbus_read_byte_data(c, S2MU106_CHG_CTRL0);
	if (mode >= 0 && (mode & REG_MODE_MASK) != REG_MODE_CHG)
		s2mu106_update(c, S2MU106_CHG_CTRL0,
			REG_MODE_CHG, REG_MODE_MASK);

	if (online >= 0 && online != chg->last_online) {
		chg->last_online = online;
		power_supply_changed(chg->psy);
	}
	schedule_delayed_work(&chg->wdog, msecs_to_jiffies(WD_KICK_MS));
}

static int s2mu106_psy_get(struct power_supply *psy,
	enum power_supply_property psp, union power_supply_propval *val)
{
	struct s2mu106_charger *chg = power_supply_get_drvdata(psy);
	struct i2c_client *c = chg->client;
	int v;

	switch (psp) {
	case POWER_SUPPLY_PROP_ONLINE:
		v = s2mu106_read_online(c);
		if (v < 0)
			return v;
		val->intval = v;
		break;
	case POWER_SUPPLY_PROP_STATUS:
		v = s2mu106_read_online(c);
		if (v < 0)
			return v;
		if (!v) {
			val->intval = POWER_SUPPLY_STATUS_DISCHARGING;
			break;
		}
		v = i2c_smbus_read_byte_data(c, S2MU106_CHG_STATUS1);
		if (v < 0)
			return v;
		val->intval = (v & DONE_STATUS_MASK) ?
			POWER_SUPPLY_STATUS_FULL : POWER_SUPPLY_STATUS_CHARGING;
		break;
	case POWER_SUPPLY_PROP_HEALTH:
		v = i2c_smbus_read_byte_data(c, S2MU106_CHG_STATUS1);
		if (v < 0)
			return v;
		val->intval = ((v & 0xf0) == 0x40) ?
			POWER_SUPPLY_HEALTH_OVERVOLTAGE : POWER_SUPPLY_HEALTH_GOOD;
		break;
	case POWER_SUPPLY_PROP_PRESENT:
		v = i2c_smbus_read_byte_data(c, S2MU106_CHG_STATUS3);
		if (v < 0)
			return v;
		val->intval = !!(v & 0x01);
		break;
	case POWER_SUPPLY_PROP_CHARGE_TYPE:
		/* vendor s2mu106_get_charge_type: STATUS3 BAT_STATUS
		 * bits[3:1] 0x2/0x3 (pre-charge) or 0x6 -> fast charging */
		v = i2c_smbus_read_byte_data(c, S2MU106_CHG_STATUS3);
		if (v < 0)
			return v;
		v = (v >> 1) & 0x7;
		val->intval = (v == 0x2 || v == 0x3 || v == 0x6) ?
			POWER_SUPPLY_CHARGE_TYPE_FAST :
			POWER_SUPPLY_CHARGE_TYPE_UNKNOWN;
		break;
	case POWER_SUPPLY_PROP_CONSTANT_CHARGE_CURRENT_MAX:
		v = i2c_smbus_read_byte_data(c, S2MU106_CHG_CTRL7);
		if (v < 0)
			return v;
		val->intval = ((v & CTRL7_FAST_MASK) + 1) * 50 * 1000;
		break;
	case POWER_SUPPLY_PROP_INPUT_CURRENT_LIMIT:
		v = i2c_smbus_read_byte_data(c, S2MU106_CHG_CTRL1);
		if (v < 0)
			return v;
		val->intval = ((v & CTRL1_INLIM_MASK) * 25 + 50) * 1000;
		break;
	default:
		return -EINVAL;
	}
	return 0;
}

static int s2mu106_psy_set(struct power_supply *psy,
	enum power_supply_property psp, const union power_supply_propval *val)
{
	struct s2mu106_charger *chg = power_supply_get_drvdata(psy);

	/* intval is in microamps */
	switch (psp) {
	case POWER_SUPPLY_PROP_INPUT_CURRENT_LIMIT:
		return s2mu106_set_input_ma(chg->client, val->intval / 1000);
	case POWER_SUPPLY_PROP_CONSTANT_CHARGE_CURRENT_MAX:
		return s2mu106_set_charge_ma(chg->client, val->intval / 1000);
	default:
		return -EINVAL;
	}
}

static int s2mu106_psy_writeable(struct power_supply *psy,
	enum power_supply_property psp)
{
	return psp == POWER_SUPPLY_PROP_INPUT_CURRENT_LIMIT ||
	       psp == POWER_SUPPLY_PROP_CONSTANT_CHARGE_CURRENT_MAX;
}

static enum power_supply_property s2mu106_psy_props[] = {
	POWER_SUPPLY_PROP_ONLINE,
	POWER_SUPPLY_PROP_STATUS,
	POWER_SUPPLY_PROP_HEALTH,
	POWER_SUPPLY_PROP_PRESENT,
	POWER_SUPPLY_PROP_CHARGE_TYPE,
	POWER_SUPPLY_PROP_CONSTANT_CHARGE_CURRENT_MAX,
	POWER_SUPPLY_PROP_INPUT_CURRENT_LIMIT,
};

static const struct power_supply_desc s2mu106_psy_desc = {
	.name		= "s2mu106-charger",
	.type		= POWER_SUPPLY_TYPE_USB,
	.properties	= s2mu106_psy_props,
	.num_properties	= ARRAY_SIZE(s2mu106_psy_props),
	.get_property	= s2mu106_psy_get,
	.set_property	= s2mu106_psy_set,
	.property_is_writeable = s2mu106_psy_writeable,
};

static int s2mu106_probe(struct i2c_client *client)
{
	struct s2mu106_charger *chg;
	struct power_supply_config cfg = {};
	int online;

	chg = devm_kzalloc(&client->dev, sizeof(*chg), GFP_KERNEL);
	if (!chg)
		return -ENOMEM;
	chg->client = client;
	i2c_set_clientdata(client, chg);

	s2mu106_program(chg);
	online = s2mu106_read_online(client);
	chg->last_online = online > 0;

	cfg.drv_data = chg;
	chg->psy = devm_power_supply_register(&client->dev,
		&s2mu106_psy_desc, &cfg);
	if (IS_ERR(chg->psy))
		return dev_err_probe(&client->dev, PTR_ERR(chg->psy),
			"power_supply register failed");

	INIT_DELAYED_WORK(&chg->wdog, s2mu106_wdog);
	schedule_delayed_work(&chg->wdog, msecs_to_jiffies(WD_KICK_MS));

	dev_info(&client->dev, "s2mu106 charger enabled (chg=%d)\n",
		chg->last_online);
	return 0;
}

static void s2mu106_remove(struct i2c_client *client)
{
	struct s2mu106_charger *chg = i2c_get_clientdata(client);

	cancel_delayed_work_sync(&chg->wdog);
}

static const struct i2c_device_id s2mu106_id[] = {
	{ "s2mu106-charger" },
	{ }
};
MODULE_DEVICE_TABLE(i2c, s2mu106_id);

static const struct of_device_id s2mu106_of_match[] = {
	{ .compatible = "samsung,s2mu106-charger" },
	{ }
};
MODULE_DEVICE_TABLE(of, s2mu106_of_match);

static struct i2c_driver s2mu106_driver = {
	.driver = {
		.name = "s2mu106-charger",
		.of_match_table = s2mu106_of_match,
	},
	.probe = s2mu106_probe,
	.remove = s2mu106_remove,
	.id_table = s2mu106_id,
};
module_i2c_driver(s2mu106_driver);

MODULE_DESCRIPTION("Minimal S2MU106 charger enable for Galaxy A21s");
MODULE_LICENSE("GPL");
