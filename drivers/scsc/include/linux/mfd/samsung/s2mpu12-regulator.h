#ifndef _COMPAT_S2MPU12_REGULATOR_H
#define _COMPAT_S2MPU12_REGULATOR_H

#include <linux/i2c.h>

static inline int s2mpu12_write_reg(struct i2c_client *i2c, u8 reg, u8 value)
{
	return 0;
}

#endif
