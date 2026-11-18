// SPDX-License-Identifier: GPL-2.0
/*
 * Samsung Galaxy A21s (SM-A217F, Exynos 850 / S5E3830) CHUB + SSP
 * sensor hub bring-up & IIO driver.
 *
 * Boots the Cortex-M4F CHUB MCU with sensorhub/shub_nacho_bl.bin +
 * sensorhub/shub_nacho_a21s.bin, establishes mailbox/SRAM IPC, and exposes
 * Gyroscope, Accelerometer, and Ambient Light Sensor (ALS) via Linux IIO.
 */

#include <linux/arm-smccc.h>
#include <linux/clk.h>
#include <linux/completion.h>
#include <linux/delay.h>
#include <linux/firmware.h>
#include <linux/iio/iio.h>
#include <linux/interrupt.h>
#include <linux/io.h>
#include <linux/irq.h>
#include <linux/irqdomain.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/slab.h>
#include <linux/string.h>
#include <linux/workqueue.h>
#include <dt-bindings/clock/exynos850.h>

/* Exynos 850 (S5E3830) CHUB hardware addresses from vendor exynos3830.dts */
#define CHUB_MAILBOX_PHYS	0x11a00000
#define CHUB_MAILBOX_SIZE	0x200
#define CHUB_SRAM_PHYS		0x10e00000
#define CHUB_SRAM_SIZE		0x40000
#define CHUB_DUMPGPR_PHYS	0x10cf0000
#define CHUB_DUMPGPR_SIZE	0x100
#define CHUB_BAAW_PHYS		0x10d20000
#define CHUB_BAAW_SIZE		0x10
#define EXYNOS850_PMU_PHYS	0x11860000
#define EXYNOS850_PMU_SIZE	0x3000

#define CHUB_MAILBOX_SPI	50	/* 0x32 */

/* PMU CHUB offsets (pmucal_chub_exynos3830.h) */
#define PMU_CHUB_CFG		0x1f80
#define PMU_CHUB_STATUS		0x1f84
#define PMU_CHUB_OPTION		0x1f8c
#define PMU_CHUB_CPU_CFG	0x2d00
#define PMU_CHUB_CPU_STATUS	0x2d04

/* SMC TZPC */
#define SMC_CMD_CONN_IF		0x82000710UL
#define EXYNOS_SHUB		2ULL
#define EXYNOS_SET_CONN_TZPC	0ULL

/* BAAW-P-APM-CHUB */
#define BAAW_START		0x40300
#define BAAW_END		0x40900
#define BAAW_REMAP		0x11900
#define BAAW_RW_ENABLE		0x80000003

/* Mailbox registers */
#define REG_MB_MCUCTL		0x000
#define REG_MB_INTGR0		0x008
#define REG_MB_INTCR0		0x00c
#define REG_MB_INTMR0		0x010
#define REG_MB_INTSR0		0x014
#define REG_MB_INTMSR0		0x018
#define REG_MB_INTGR1		0x01c
#define REG_MB_INTCR1		0x020
#define REG_MB_INTMR1		0x024
#define REG_MB_INTSR1		0x028
#define REG_MB_INTMSR1		0x02c
#define REG_MB_ISSR0		0x080

/* CHUB bootargs & IPC layout (chub_ipc.h) */
#define MAP_INFO_OFFSET		256
#define OS_UPDT_MAGIC		"Nanohub OS"
#define CHUB_IPC_MAGIC		"IPC magic"
#define IPC_VERSION		190128
#define BOOTMODE_COLD		0x7733

#define PACKET_SIZE_MAX		777
#define IPC_CH_BUF_NUM		16
#define IPC_EVT_NUM		50
#define IRQ_MAX			16
#define CHUB_PERSISTBUF_SIZE	96
#define LOGBUF_SIZE		84
#define LOGBUF_NUM		80
#define LOGBUF_DATA_SIZE	(LOGBUF_SIZE - sizeof(u64) - sizeof(u16))

#define IRQ_EVT_CH0		0
#define IRQ_EVT_C2A_LOG		14
#define IRQ_EVT_CHUB_ALIVE	15
#define IRQ_EVT_CHUB_MAX	16

/* SSP commands & sensor types (ssp_cmd_define.h / ssp_type_define.h) */
#define SSP_CMD_ADD		0
#define SSP_CMD_REMOVE		1
#define SSP_CMD_CHANGERATE	2
#define SSP_CMD_SETVALUE	3
#define SSP_CMD_GETVALUE	4
#define SSP_CMD_REPORT		5

#define SSP_SUB_SENSOR_SCAN	27
#define SSP_MSG_HEADER_SIZE	13

#define SENSOR_TYPE_ACCEL	1
#define SENSOR_TYPE_MAG		2
#define SENSOR_TYPE_GYRO	4
#define SENSOR_TYPE_LIGHT	5
#define SENSOR_TYPE_PROX	8

static bool use_ccf_chubclk;
module_param(use_ccf_chubclk, bool, 0444);
MODULE_PARM_DESC(use_ccf_chubclk,
		 "Write the CCF-reported CHUB bus rate to the firmware boot arguments");

struct chub_bootargs {
	char magic[16];
	u32 ipc_version;
	u32 bl_start;
	u32 bl_end;
	u32 code_start;
	u32 code_end;
	u32 ipc_start;
	u32 ipc_end;
	u32 ram_start;
	u32 ram_end;
	u32 shared_start;
	u32 shared_end;
	u32 dump_start;
	u32 dump_end;
	u32 chubclk;
	u16 bootmode;
	u16 runtimelog;
	u32 psp;
	u32 msp;
};

struct ipc_channel_buf {
	u32 size;
	u8 buf[PACKET_SIZE_MAX];
};

struct ipc_buf {
	volatile u32 eq;
	volatile u32 dq;
	volatile u32 full;
	volatile u32 empty;
	struct ipc_channel_buf ch[IPC_CH_BUF_NUM];
};

struct ipc_evt_ctrl {
	u32 eq;
	u32 dq;
	u32 full;
	u32 empty;
	u32 irq;
	bool pending[IRQ_MAX];
};

struct ipc_evt_buf {
	u32 evt;
	u32 irq;
	u32 status;
};

struct ipc_evt {
	struct ipc_evt_buf data[IPC_EVT_NUM];
	struct ipc_evt_ctrl ctrl;
};

struct ipc_debug {
	u32 event;
	u32 val[2];
};

struct sensor_map {
	char magic[16];
	int index;
	u8 active_sensor_list[36];
};

struct logbuf_content {
	char buf[LOGBUF_DATA_SIZE];
	u8 error;
	u8 newline;
	u32 size;
	u32 nextaddr;
};

struct logbuf_raw {
	u32 eq;
	u32 dq;
	u32 size;
	u32 full;
	char buf[];
};

struct ipc_logbuf {
	struct logbuf_content log[LOGBUF_NUM];
	u32 eq;
	u32 dq;
	u32 size;
	u8 full;
	u8 flush_req;
	u8 flush_active;
	u8 loglevel;
	int dbg_full_cnt;
	int errcnt;
	int reqcnt;
	u32 fw_num;
	struct logbuf_raw logbuf;
};

struct ipc_map_area {
	char persist[CHUB_PERSISTBUF_SIZE];
	char magic[16];
	u16 wake;
	struct ipc_buf data[2];	/* 0 = C2A, 1 = A2C */
	struct ipc_evt evt[2];	/* 0 = C2A, 1 = A2C */
	struct ipc_debug dbg;
	struct sensor_map sensormap;
	struct ipc_logbuf logbuf;
};

struct ssp_msg_hdr {
	u8 cmd;
	u8 type;
	u8 subcmd;
	u16 length;
	u64 timestamp;
} __packed;

struct a21s_chub_dev {
	struct platform_device *pdev;
	void __iomem *mailbox;
	void __iomem *sram;
	void __iomem *dumpgpr;
	void __iomem *baaw;
	void __iomem *pmu;
	int irq;

	struct chub_bootargs *bootargs;
	struct ipc_map_area *ipc_map;

	struct completion alive_done;
	bool alive;

	struct work_struct rx_work;
	struct mutex tx_lock;

	/* Cached latest sensor readings */
	s16 accel[3];
	s16 gyro[3];
	u32 light_lux;
	u16 prox;
	bool accel_valid;
	bool gyro_valid;
	bool light_valid;

	struct iio_dev *indio_gyro;
	struct iio_dev *indio_accel;
	struct iio_dev *indio_light;
};

static struct a21s_chub_dev *g_chub;

static int pmu_wait_bit(void __iomem *pmu, u32 reg, u32 bit, bool set)
{
	int i;
	u32 val;

	for (i = 0; i < 1000; i++) {
		val = readl(pmu + reg);
		if (set ? (val & bit) : !(val & bit))
			return 0;
		udelay(10);
	}
	return -ETIMEDOUT;
}

static void pmu_update_bit(void __iomem *pmu, u32 reg, u32 mask, u32 val)
{
	u32 cur = readl(pmu + reg);

	cur = (cur & ~mask) | (val & mask);
	writel(cur, pmu + reg);
}

static int chub_pmu_reset_release_config(struct a21s_chub_dev *c)
{
	pmu_update_bit(c->pmu, PMU_CHUB_CFG, BIT(0), BIT(0));
	if (pmu_wait_bit(c->pmu, PMU_CHUB_STATUS, BIT(0), true))
		return -ETIMEDOUT;
	pmu_update_bit(c->pmu, PMU_CHUB_OPTION, BIT(0), 0);
	return 0;
}

static int chub_pmu_reset_assert(struct a21s_chub_dev *c)
{
	pmu_update_bit(c->pmu, PMU_CHUB_OPTION, BIT(0), BIT(0));
	pmu_update_bit(c->pmu, PMU_CHUB_CFG, BIT(0), 0);
	return pmu_wait_bit(c->pmu, PMU_CHUB_STATUS, BIT(0), false);
}

static int chub_pmu_reset_release(struct a21s_chub_dev *c)
{
	pmu_update_bit(c->pmu, PMU_CHUB_CPU_CFG, BIT(0), BIT(0));
	return pmu_wait_bit(c->pmu, PMU_CHUB_CPU_STATUS, BIT(0), true);
}

static int chub_map_gic_spi(u32 spi_num)
{
	struct device_node *gic_np;
	struct irq_domain *dom;
	struct irq_fwspec fwspec;

	gic_np = of_find_compatible_node(NULL, NULL, "arm,gic-400");
	if (!gic_np)
		gic_np = of_find_compatible_node(NULL, NULL, "arm,gic-v3");
	if (!gic_np)
		return -ENODEV;

	dom = irq_find_host(gic_np);
	of_node_put(gic_np);
	if (!dom)
		return -ENODEV;

	fwspec.fwnode = dom->fwnode;
	fwspec.param_count = 3;
	fwspec.param[0] = 0;	/* GIC_SPI */
	fwspec.param[1] = spi_num;
	fwspec.param[2] = IRQ_TYPE_LEVEL_HIGH;

	return irq_create_fwspec_mapping(&fwspec);
}

static void chub_parse_ssp_report(struct a21s_chub_dev *c, const u8 *pkt, u16 len)
{
	u16 idx = 0;

	while (idx < len) {
		u8 tag = pkt[idx++];

		if (tag == 0x37 /* SSP2AP_BYPASS_DATA */) {
			u8 sensor_type;
			u16 count, i;

			if (idx + 3 > len)
				break;
			sensor_type = pkt[idx++];
			count = pkt[idx] | (pkt[idx + 1] << 8);
			idx += 2;

			for (i = 0; i < count; i++) {
				switch (sensor_type) {
				case SENSOR_TYPE_ACCEL:
					if (idx + 6 + 8 > len)
						return;
					c->accel[0] = (s16)(pkt[idx] | (pkt[idx + 1] << 8));
					c->accel[1] = (s16)(pkt[idx + 2] | (pkt[idx + 3] << 8));
					c->accel[2] = (s16)(pkt[idx + 4] | (pkt[idx + 5] << 8));
					c->accel_valid = true;
					idx += 6 + 8;
					break;
				case SENSOR_TYPE_GYRO:
					if (idx + 6 + 8 > len)
						return;
					c->gyro[0] = (s16)(pkt[idx] | (pkt[idx + 1] << 8));
					c->gyro[1] = (s16)(pkt[idx + 2] | (pkt[idx + 3] << 8));
					c->gyro[2] = (s16)(pkt[idx + 4] | (pkt[idx + 5] << 8));
					c->gyro_valid = true;
					idx += 6 + 8;
					break;
				case SENSOR_TYPE_LIGHT:
					if (idx + 28 + 8 > len)
						return;
					c->light_lux = (u32)(pkt[idx] | (pkt[idx + 1] << 8) |
							     (pkt[idx + 2] << 16) | (pkt[idx + 3] << 24));
					c->light_valid = true;
					idx += 28 + 8;
					break;
				default:
					return;
				}
			}
		} else {
			pr_info("a21s-chub: report tag=0x%02x len=%u data=%*ph\n",
				tag, len, min_t(u32, len, 24), pkt);
			break;
		}
	}
}

static void chub_rx_work(struct work_struct *work)
{
	struct a21s_chub_dev *c = container_of(work, struct a21s_chub_dev, rx_work);
	struct ipc_buf *c2a = &c->ipc_map->data[0];
	struct ipc_evt *evt_c2a = &c->ipc_map->evt[0];
	u8 buf[PACKET_SIZE_MAX];

	/* Drain C2A event queue */
	while (evt_c2a->ctrl.dq != evt_c2a->ctrl.eq) {
		struct ipc_evt_buf *ev = &evt_c2a->data[evt_c2a->ctrl.dq];

		evt_c2a->ctrl.pending[ev->irq] = false;
		evt_c2a->ctrl.dq = (evt_c2a->ctrl.dq + 1) % IPC_EVT_NUM;
	}

	/* Drain C2A data queue */
	while (c2a->dq != c2a->eq && c2a->dq < IPC_CH_BUF_NUM) {
		struct ipc_channel_buf *ch = &c2a->ch[c2a->dq];
		u32 sz = ch->size;

		if (sz > 0 && sz <= PACKET_SIZE_MAX) {
			memcpy_fromio(buf, ch->buf, sz);
			if (sz >= SSP_MSG_HEADER_SIZE) {
				u8 cmd = buf[0];
				u8 type = buf[1];
				u8 subcmd = buf[2];
				u16 plen = (u16)buf[3] | ((u16)buf[4] << 8);

				if (cmd == SSP_CMD_REPORT &&
				    SSP_MSG_HEADER_SIZE + plen <= sz) {
					chub_parse_ssp_report(c, buf + SSP_MSG_HEADER_SIZE, plen);
				} else {
					pr_info("a21s-chub: rx cmd=%u type=%u sub=%u plen=%u data=%*ph\n",
						cmd, type, subcmd, plen,
						min_t(u32, plen, 32),
						buf + SSP_MSG_HEADER_SIZE);
				}
			}
		}
		c2a->dq = (c2a->dq + 1) % IPC_CH_BUF_NUM;
	}
}

static irqreturn_t chub_mailbox_irq(int irq, void *data)
{
	struct a21s_chub_dev *c = data;
	u32 status = readl(c->mailbox + REG_MB_INTSR0);
	u32 alive_bit = BIT(IRQ_EVT_CHUB_ALIVE + IRQ_EVT_CHUB_MAX);
	u32 log_bit = BIT(IRQ_EVT_C2A_LOG + IRQ_EVT_CHUB_MAX);

	if (status & alive_bit) {
		writel(alive_bit, c->mailbox + REG_MB_INTCR0);
		c->alive = true;
		complete(&c->alive_done);
		status &= ~alive_bit;
	}

	if (status & log_bit) {
		writel(log_bit, c->mailbox + REG_MB_INTCR0);
		status &= ~log_bit;
	}

	if (status) {
		writel(status, c->mailbox + REG_MB_INTCR0);
		schedule_work(&c->rx_work);
	}

	return IRQ_HANDLED;
}

static int chub_send_ssp_cmd(struct a21s_chub_dev *c, u8 cmd, u8 type,
			     u8 subcmd, const u8 *payload, u16 len)
{
	struct ipc_buf *a2c = &c->ipc_map->data[1];
	struct ipc_evt *evt_a2c = &c->ipc_map->evt[1];
	struct ipc_channel_buf *ch;
	struct ipc_evt_buf *ev;
	u8 pkt[64];
	u32 next_eq;

	if (SSP_MSG_HEADER_SIZE + len > sizeof(pkt))
		return -EINVAL;

	memset(pkt, 0, sizeof(pkt));
	pkt[0] = cmd;
	pkt[1] = type;
	pkt[2] = subcmd;
	pkt[3] = len & 0xff;
	pkt[4] = (len >> 8) & 0xff;
	if (len && payload)
		memcpy(pkt + SSP_MSG_HEADER_SIZE, payload, len);

	mutex_lock(&c->tx_lock);
	next_eq = (a2c->eq + 1) % IPC_CH_BUF_NUM;
	if (next_eq == a2c->dq) {
		mutex_unlock(&c->tx_lock);
		return -EBUSY;
	}

	ch = &a2c->ch[a2c->eq];
	ch->size = SSP_MSG_HEADER_SIZE + len;
	memcpy_toio(ch->buf, pkt, ch->size);
	wmb();
	a2c->eq = next_eq;

	ev = &evt_a2c->data[evt_a2c->ctrl.eq];
	ev->evt = IRQ_EVT_CH0;
	ev->irq = IRQ_EVT_CH0;
	evt_a2c->ctrl.pending[IRQ_EVT_CH0] = true;
	evt_a2c->ctrl.eq = (evt_a2c->ctrl.eq + 1) % IPC_EVT_NUM;
	wmb();

	writel(BIT(IRQ_EVT_CH0), c->mailbox + REG_MB_INTGR1);
	mutex_unlock(&c->tx_lock);
	msleep(20);
	return 0;
}

/* ---- IIO interfaces -------------------------------------------------- */

static int a21s_gyro_read_raw(struct iio_dev *indio_dev,
			      struct iio_chan_spec const *chan,
			      int *val, int *val2, long mask)
{
	struct a21s_chub_dev *c = *(struct a21s_chub_dev **)iio_priv(indio_dev);

	if (mask == IIO_CHAN_INFO_RAW) {
		if (chan->channel2 < IIO_MOD_X || chan->channel2 > IIO_MOD_Z)
			return -EINVAL;
		if (!c->gyro_valid)
			return -ENODATA;
		*val = c->gyro[chan->channel2 - IIO_MOD_X];
		return IIO_VAL_INT;
	}
	if (mask == IIO_CHAN_INFO_SCALE) {
		/* LSM6DSL +/-2000 dps -> ~0.07 rad/s / 1000 */
		*val = 0;
		*val2 = 1221730;
		return IIO_VAL_INT_PLUS_NANO;
	}
	return -EINVAL;
}

static const struct iio_chan_spec a21s_gyro_channels[] = {
	{ .type = IIO_ANGL_VEL, .modified = 1, .channel2 = IIO_MOD_X,
	  .info_mask_separate = BIT(IIO_CHAN_INFO_RAW),
	  .info_mask_shared_by_type = BIT(IIO_CHAN_INFO_SCALE) },
	{ .type = IIO_ANGL_VEL, .modified = 1, .channel2 = IIO_MOD_Y,
	  .info_mask_separate = BIT(IIO_CHAN_INFO_RAW),
	  .info_mask_shared_by_type = BIT(IIO_CHAN_INFO_SCALE) },
	{ .type = IIO_ANGL_VEL, .modified = 1, .channel2 = IIO_MOD_Z,
	  .info_mask_separate = BIT(IIO_CHAN_INFO_RAW),
	  .info_mask_shared_by_type = BIT(IIO_CHAN_INFO_SCALE) },
};

static const struct iio_info a21s_gyro_info = {
	.read_raw = a21s_gyro_read_raw,
};

static int a21s_accel_read_raw(struct iio_dev *indio_dev,
			       struct iio_chan_spec const *chan,
			       int *val, int *val2, long mask)
{
	struct a21s_chub_dev *c = *(struct a21s_chub_dev **)iio_priv(indio_dev);

	if (mask == IIO_CHAN_INFO_RAW) {
		if (chan->channel2 < IIO_MOD_X || chan->channel2 > IIO_MOD_Z)
			return -EINVAL;
		if (!c->accel_valid)
			return -ENODATA;
		*val = c->accel[chan->channel2 - IIO_MOD_X];
		return IIO_VAL_INT;
	}
	if (mask == IIO_CHAN_INFO_SCALE) {
		*val = 0;
		*val2 = 598550;
		return IIO_VAL_INT_PLUS_NANO;
	}
	return -EINVAL;
}

static const struct iio_chan_spec a21s_accel_channels[] = {
	{ .type = IIO_ACCEL, .modified = 1, .channel2 = IIO_MOD_X,
	  .info_mask_separate = BIT(IIO_CHAN_INFO_RAW),
	  .info_mask_shared_by_type = BIT(IIO_CHAN_INFO_SCALE) },
	{ .type = IIO_ACCEL, .modified = 1, .channel2 = IIO_MOD_Y,
	  .info_mask_separate = BIT(IIO_CHAN_INFO_RAW),
	  .info_mask_shared_by_type = BIT(IIO_CHAN_INFO_SCALE) },
	{ .type = IIO_ACCEL, .modified = 1, .channel2 = IIO_MOD_Z,
	  .info_mask_separate = BIT(IIO_CHAN_INFO_RAW),
	  .info_mask_shared_by_type = BIT(IIO_CHAN_INFO_SCALE) },
};

static const struct iio_info a21s_accel_info = {
	.read_raw = a21s_accel_read_raw,
};

static int a21s_light_read_raw(struct iio_dev *indio_dev,
			       struct iio_chan_spec const *chan,
			       int *val, int *val2, long mask)
{
	struct a21s_chub_dev *c = *(struct a21s_chub_dev **)iio_priv(indio_dev);

	if (mask == IIO_CHAN_INFO_PROCESSED || mask == IIO_CHAN_INFO_RAW) {
		if (!c->light_valid)
			return -ENODATA;
		*val = (int)c->light_lux;
		return IIO_VAL_INT;
	}
	return -EINVAL;
}

static const struct iio_chan_spec a21s_light_channels[] = {
	{ .type = IIO_LIGHT,
	  .info_mask_separate = BIT(IIO_CHAN_INFO_RAW) |
				BIT(IIO_CHAN_INFO_PROCESSED) },
};

static const struct iio_info a21s_light_info = {
	.read_raw = a21s_light_read_raw,
};

static struct iio_dev *chub_reg_iio(struct a21s_chub_dev *c, const char *name,
				    const struct iio_chan_spec *chans,
				    int nchans, const struct iio_info *info)
{
	struct iio_dev *indio;
	struct a21s_chub_dev **priv;
	int ret;

	indio = devm_iio_device_alloc(&c->pdev->dev, sizeof(c));
	if (!indio)
		return ERR_PTR(-ENOMEM);

	priv = iio_priv(indio);
	*priv = c;

	indio->name = name;
	indio->info = info;
	indio->modes = INDIO_DIRECT_MODE;
	indio->channels = chans;
	indio->num_channels = nchans;

	ret = devm_iio_device_register(&c->pdev->dev, indio);
	if (ret)
		return ERR_PTR(ret);
	return indio;
}

static int __init chub_configure_boot_clock(struct a21s_chub_dev *c)
{
	struct of_phandle_args spec = {
		.args_count = 1,
		.args = { CLK_DOUT_CLKCMU_CHUB_BUS },
	};
	struct clk *clk;
	unsigned long rate = 0;
	u32 original = readl(&c->bootargs->chubclk);
	int ret = -ENODEV;

	/* The experimental platform device has no clock consumer node. */
	spec.np = of_find_compatible_node(NULL, NULL, "samsung,exynos850-cmu-apm");
	if (spec.np) {
		clk = of_clk_get_from_provider(&spec);
		of_node_put(spec.np);
		if (IS_ERR(clk)) {
			ret = PTR_ERR(clk);
		} else {
			rate = clk_get_rate(clk);
			clk_put(clk);
			ret = !rate || rate > U32_MAX ? -ERANGE : 0;
		}
	}

	pr_info("a21s-chub: boot clock mode=%c ccf_hz=%lu bootargs_original_hz=%u lookup_rc=%d\n",
		use_ccf_chubclk ? 'B' : 'A', rate, original, ret);
	if (use_ccf_chubclk) {
		if (ret)
			return ret;
		writel((u32)rate, &c->bootargs->chubclk);
	}
	pr_info("a21s-chub: bootargs_effective_hz=%u\n",
		readl(&c->bootargs->chubclk));
	return 0;
}

static int __init a21s_chub_init(void)
{
	struct a21s_chub_dev *c;
	const struct firmware *bl_fw = NULL, *os_fw = NULL;
	struct arm_smccc_res smc_res;
	u8 enable_payload[9] = { 0 };
	int ret, i;

	c = kzalloc(sizeof(*c), GFP_KERNEL);
	if (!c)
		return -ENOMEM;

	init_completion(&c->alive_done);
	INIT_WORK(&c->rx_work, chub_rx_work);
	mutex_init(&c->tx_lock);

	c->pdev = platform_device_register_simple("a21s-chub", -1, NULL, 0);
	if (IS_ERR(c->pdev)) {
		ret = PTR_ERR(c->pdev);
		goto err_free;
	}

	c->mailbox = ioremap(CHUB_MAILBOX_PHYS, CHUB_MAILBOX_SIZE);
	c->sram = ioremap_wc(CHUB_SRAM_PHYS, CHUB_SRAM_SIZE);
	c->dumpgpr = ioremap(CHUB_DUMPGPR_PHYS, CHUB_DUMPGPR_SIZE);
	c->baaw = ioremap(CHUB_BAAW_PHYS, CHUB_BAAW_SIZE);
	c->pmu = ioremap(EXYNOS850_PMU_PHYS, EXYNOS850_PMU_SIZE);
	if (!c->mailbox || !c->sram || !c->dumpgpr || !c->baaw || !c->pmu) {
		ret = -ENOMEM;
		goto err_unmap;
	}

	ret = request_firmware(&bl_fw, "sensorhub/shub_nacho_bl.bin", &c->pdev->dev);
	if (ret) {
		pr_err("a21s-chub: request_firmware(shub_nacho_bl.bin) failed: %d\n", ret);
		goto err_unmap;
	}
	ret = request_firmware(&os_fw, "sensorhub/shub_nacho_a21s.bin", &c->pdev->dev);
	if (ret) {
		pr_err("a21s-chub: request_firmware(shub_nacho_a21s.bin) failed: %d\n", ret);
		goto err_fw;
	}

	c->irq = chub_map_gic_spi(CHUB_MAILBOX_SPI);
	if (c->irq <= 0) {
		pr_err("a21s-chub: failed to map GIC SPI %d: %d\n",
		       CHUB_MAILBOX_SPI, c->irq);
		ret = c->irq ? c->irq : -ENODEV;
		goto err_fw;
	}

	g_chub = c;
	ret = request_irq(c->irq, chub_mailbox_irq, 0, "a21s-chub-mb", c);
	if (ret) {
		pr_err("a21s-chub: request_irq(%d) failed: %d\n", c->irq, ret);
		goto err_fw;
	}

	/* 1. Reset assert & prepare */
	chub_pmu_reset_assert(c);
	ret = chub_pmu_reset_release_config(c);
	if (ret) {
		pr_err("a21s-chub: pmu reset_release_config failed: %d\n", ret);
		goto err_irq;
	}

	arm_smccc_smc(SMC_CMD_CONN_IF, (EXYNOS_SHUB << 32) | EXYNOS_SET_CONN_TZPC,
		      0, 0, 0, 0, 0, 0, &smc_res);
	if (smc_res.a0)
		pr_warn("a21s-chub: SMC TZPC returned %lu\n", smc_res.a0);

	/* 2. BAAW-P-APM-CHUB */
	writel(BAAW_START, c->baaw + 0x0);
	writel(BAAW_END, c->baaw + 0x4);
	writel(BAAW_REMAP, c->baaw + 0x8);
	writel(BAAW_RW_ENABLE, c->baaw + 0xc);

	/* 3. Clear SRAM & load bootloader */
	memset_io(c->sram, 0, CHUB_SRAM_SIZE);
	memcpy_toio(c->sram, bl_fw->data, bl_fw->size);

	c->bootargs = (struct chub_bootargs *)(c->sram + MAP_INFO_OFFSET);
	if (strncmp(OS_UPDT_MAGIC, c->bootargs->magic, strlen(OS_UPDT_MAGIC)) ||
	    c->bootargs->ipc_version != IPC_VERSION) {
		pr_err("a21s-chub: invalid bootargs magic/ver (ver=%u)\n",
		       c->bootargs->ipc_version);
		ret = -EINVAL;
		goto err_irq;
	}

	/* 4. Load OS image & init IPC map */
	memcpy_toio(c->sram + c->bootargs->code_start, os_fw->data, os_fw->size);
	c->bootargs->bootmode = BOOTMODE_COLD;
	c->bootargs->runtimelog = 2;
	ret = chub_configure_boot_clock(c);
	if (ret)
		goto err_irq;

	c->ipc_map = (struct ipc_map_area *)(c->sram + c->bootargs->ipc_start);
	memcpy(c->ipc_map->magic, CHUB_IPC_MAGIC, sizeof(CHUB_IPC_MAGIC));
	c->ipc_map->logbuf.size = LOGBUF_SIZE * LOGBUF_NUM;
	c->ipc_map->logbuf.logbuf.size =
		(c->bootargs->ipc_end - c->bootargs->ipc_start) -
		sizeof(struct ipc_map_area);
	c->ipc_map->wake = 1;

	writel(0xffffffff, c->mailbox + REG_MB_INTCR0);
	writel(0xffffffff, c->mailbox + REG_MB_INTCR1);
	writel(0, c->mailbox + REG_MB_ISSR0);	/* SR_BOOT_MODE = 0 */
	writel(1, c->dumpgpr + 0x0);		/* DUMPGPR enable */
	wmb();

	/* 5. Release CHUB CPU */
	ret = chub_pmu_reset_release(c);
	if (ret) {
		pr_err("a21s-chub: pmu reset_release failed: %d\n", ret);
		goto err_irq;
	}

	/* 6. Alive handshake */
	for (i = 0; i < 10; i++) {
		writel(BIT(IRQ_EVT_CHUB_ALIVE), c->mailbox + REG_MB_INTGR1);
		if (wait_for_completion_timeout(&c->alive_done, msecs_to_jiffies(100)))
			break;
	}

	if (!c->alive) {
		pr_err("a21s-chub: CHUB alive timeout (INTSR0=0x%x INTSR1=0x%x)\n",
		       readl(c->mailbox + REG_MB_INTSR0),
		       readl(c->mailbox + REG_MB_INTSR1));
		ret = -ETIMEDOUT;
		goto err_irq;
	}

	pr_info("a21s-chub: CHUB ALIVE! (code=0x%x..0x%x ipc=0x%x)\n",
		c->bootargs->code_start, c->bootargs->code_end,
		c->bootargs->ipc_start);

	/* Query sensor scan & enable accel/gyro/light at 20ms */
	chub_send_ssp_cmd(c, SSP_CMD_SETVALUE, 255 /* TYPE_MCU */, 1 /* AP_RESUME */, NULL, 0);
	chub_send_ssp_cmd(c, SSP_CMD_SETVALUE, 255 /* TYPE_MCU */, 5 /* LCD_ON */, NULL, 0);
	chub_send_ssp_cmd(c, SSP_CMD_GETVALUE, 255 /* TYPE_MCU */, SSP_SUB_SENSOR_SCAN, NULL, 0);
	enable_payload[0] = 20; /* 20 ms sampling */
	chub_send_ssp_cmd(c, SSP_CMD_ADD, SENSOR_TYPE_ACCEL, 0, enable_payload, 8);
	chub_send_ssp_cmd(c, SSP_CMD_ADD, SENSOR_TYPE_GYRO, 0, enable_payload, 8);
	chub_send_ssp_cmd(c, SSP_CMD_ADD, SENSOR_TYPE_LIGHT, 0, enable_payload, 8);

	msleep(200);
	pr_info("a21s-chub: INTSR0=0x%x INTSR1=0x%x a2c=%u/%u c2a=%u/%u evt_c2a=%u/%u rawlog=%u/%u\n",
		readl(c->mailbox + REG_MB_INTSR0),
		readl(c->mailbox + REG_MB_INTSR1),
		c->ipc_map->data[1].eq, c->ipc_map->data[1].dq,
		c->ipc_map->data[0].eq, c->ipc_map->data[0].dq,
		c->ipc_map->evt[0].ctrl.eq, c->ipc_map->evt[0].ctrl.dq,
		c->ipc_map->logbuf.logbuf.eq, c->ipc_map->logbuf.logbuf.dq);
	if (c->ipc_map->logbuf.logbuf.eq > 0 && c->ipc_map->logbuf.logbuf.eq < 4096) {
		u32 rlen = min_t(u32, c->ipc_map->logbuf.logbuf.eq, 512);
		pr_info("a21s-chub rawlog: %.*s\n", (int)rlen, c->ipc_map->logbuf.logbuf.buf);
	}
	for (i = 0; i < LOGBUF_NUM && c->ipc_map->logbuf.dq != c->ipc_map->logbuf.eq; i++) {
		u32 dq = c->ipc_map->logbuf.dq;
		if (dq >= LOGBUF_NUM)
			break;
		pr_info("a21s-chub fw: %.*s\n",
			(int)min_t(u32, c->ipc_map->logbuf.log[dq].size, LOGBUF_DATA_SIZE),
			c->ipc_map->logbuf.log[dq].buf);
		c->ipc_map->logbuf.dq = (dq + 1) % LOGBUF_NUM;
	}

	c->indio_gyro = chub_reg_iio(c, "a21s-gyro", a21s_gyro_channels,
				     ARRAY_SIZE(a21s_gyro_channels), &a21s_gyro_info);
	c->indio_accel = chub_reg_iio(c, "a21s-accel", a21s_accel_channels,
				      ARRAY_SIZE(a21s_accel_channels), &a21s_accel_info);
	c->indio_light = chub_reg_iio(c, "a21s-light", a21s_light_channels,
				      ARRAY_SIZE(a21s_light_channels), &a21s_light_info);

	release_firmware(os_fw);
	release_firmware(bl_fw);
	return 0;

err_irq:
	free_irq(c->irq, c);
err_fw:
	if (os_fw)
		release_firmware(os_fw);
	if (bl_fw)
		release_firmware(bl_fw);
err_unmap:
	if (c->pmu)
		iounmap(c->pmu);
	if (c->baaw)
		iounmap(c->baaw);
	if (c->dumpgpr)
		iounmap(c->dumpgpr);
	if (c->sram)
		iounmap(c->sram);
	if (c->mailbox)
		iounmap(c->mailbox);
	if (c->pdev && !IS_ERR(c->pdev))
		platform_device_unregister(c->pdev);
err_free:
	g_chub = NULL;
	kfree(c);
	return ret;
}

static void __exit a21s_chub_exit(void)
{
	struct a21s_chub_dev *c = g_chub;

	if (!c)
		return;

	cancel_work_sync(&c->rx_work);
	if (c->irq > 0)
		free_irq(c->irq, c);
	chub_pmu_reset_assert(c);

	if (c->pdev)
		platform_device_unregister(c->pdev);
	if (c->pmu)
		iounmap(c->pmu);
	if (c->baaw)
		iounmap(c->baaw);
	if (c->dumpgpr)
		iounmap(c->dumpgpr);
	if (c->sram)
		iounmap(c->sram);
	if (c->mailbox)
		iounmap(c->mailbox);

	g_chub = NULL;
	kfree(c);
}

module_init(a21s_chub_init);
module_exit(a21s_chub_exit);

MODULE_DESCRIPTION("Samsung Galaxy A21s CHUB/SSP sensor hub driver");
MODULE_LICENSE("GPL");
