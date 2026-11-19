#ifndef _COMPAT_EXYNOS_SOC_H
#define _COMPAT_EXYNOS_SOC_H

struct exynos_soc_info_s {
	u32 product_id;
	u32 revision;
	u64 unique_id;
};

#ifndef EXYNOS_SOC_UNIQUE_ID
#define EXYNOS_SOC_UNIQUE_ID 0x00E383000001ULL
#endif

static const struct exynos_soc_info_s exynos_soc_info = {
	.product_id = 0xE3830000,
	.revision = 0,
	.unique_id = EXYNOS_SOC_UNIQUE_ID,
};

#endif
