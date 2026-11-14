#ifndef _COMPAT_LINUX_SMC_H
#define _COMPAT_LINUX_SMC_H

#include <linux/arm-smccc.h>

#ifndef SMC_CMD_CONN_IF
#define SMC_CMD_CONN_IF (0x82000710)
#endif

static inline unsigned long exynos_smc(unsigned long cmd, unsigned long arg0,
				       unsigned long arg1, unsigned long arg2)
{
	struct arm_smccc_res res;

	arm_smccc_smc(cmd, arg0, arg1, arg2, 0, 0, 0, 0, &res);
	return res.a0;
}

#endif
