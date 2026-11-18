#ifndef _COMPAT_EXYNOS_SOC_H
#define _COMPAT_EXYNOS_SOC_H

struct exynos_soc_info_s {
	u32 product_id;
	u32 revision;
	u64 unique_id;
};

static const struct exynos_soc_info_s exynos_soc_info = {
	.product_id = 0xE3830000,
	.revision = 0,
	.unique_id = 0x00A217F00001ULL,
};

#endif
