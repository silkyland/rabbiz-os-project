// SPDX-License-Identifier: GPL-2.0
/*
 * /dev/umts_boot0 - CP bootloader/firmware interface.
 *
 * Implements the vendor bootdump_io_device.c ioctl surface so that a
 * userspace loader (tools/cbd in this repo, or vendor cbd) can:
 *
 *   1. CPIF_IOC_POWER_ON           - power up + init CP
 *   2. CPIF_IOC_LOAD_CP_IMAGE      - stream modem.bin into CP RAM
 *   3. CPIF_IOC_REQ_SECURITY       - request secure-boot check (SMC)
 *   4. CPIF_IOC_START_CP_BOOTLOADER- release CP out of reset
 *   5. CPIF_IOC_GET_CP_STATUS      - poll modem state
 *   6. CPIF_IOC_COMPLETE_NORMAL_BOOTUP
 *
 * After START_CP_BOOTLOADER, userspace exchanges the UDL control words
 * 900d/a00d and 9f00/af00 through read/write before waiting for PHONE_START.
 * Only these four-byte boot controls are supported, not general UDL transfers.
 */
#include <linux/fs.h>
#include <linux/io.h>
#include <linux/delay.h>
#include <linux/miscdevice.h>
#include <linux/slab.h>
#include <linux/uaccess.h>
#include <linux/unaligned.h>

#include "cpif.h"

#define CPIF_BOOT_DEV_NAME	"umts_boot0"

#define CPIF_SMC_ID_CLK		0x82001011
#define CPIF_SSS_CLK_ENABLE	0
#define CPIF_SSS_CLK_DISABLE	1
#define CPIF_SMC_MODE_CHECK_CONT	0x8
#define CPIF_SMC_SIGN_NOT_FINISH	0x100
#define CPIF_SMC_MAX_TRY	0x1000

/* ap2cp_united_status / cp2ap_united_status fields (vendor DT sbi_*) */
#define SBI_AP_STATUS_MASK	0xf
#define SBI_AP_STATUS_POS	1
#define SBI_PDA_ACTIVE_MASK	0x1
#define SBI_PDA_ACTIVE_POS	5
#define SBI_SYS_REV_MASK	0xff
#define SBI_SYS_REV_POS		6
#define SBI_DS_DET_MASK		0x3
#define SBI_DS_DET_POS		14
#define SBI_CP_STATUS_MASK	0xf
#define SBI_CP_STATUS_POS	1

#define CPIF_INIT_TIMEOUT	(15 * HZ)

/* Legacy shmem link bring-up, translated from vendor
 * init_legacy_link() + link_start_normal_boot() (SIPC protocol, DPRAM
 * magic attr set in mif,link_attrs=0x7c9): the CP bootloader polls the
 * IPC region's mem_access/magic words before it loads MAIN, so this must
 * run before AP2CP_CFG is raised. */

#define CPIF_MEM_BOOT_MAGIC	0x424f4f54	/* "BOOT" */
#define CPIF_IPC_MEM_ACCESS_OFF	0x4
#define CPIF_IPC_FMT_HT_OFF	0x08	/* fmt tx/rx head+tail */
#define CPIF_IPC_RAW_HT_OFF	0x18	/* raw tx/rx head+tail */

static int cpif_legacy_link_init(struct cpif_dev *c)
{
	void __iomem *ipc = c->shmem[CPIF_SHMEM_IPC].map;
	static const u32 ht_off[] = { CPIF_IPC_FMT_HT_OFF, CPIF_IPC_RAW_HT_OFF };
	int i;

	if (!ipc)
		return -ENODEV;

	writel(0, ipc);
	writel(0, ipc + CPIF_IPC_MEM_ACCESS_OFF);
	for (i = 0; i < ARRAY_SIZE(ht_off); i++) {
		writel(0, ipc + ht_off[i] + 0);
		writel(0, ipc + ht_off[i] + 4);
		writel(0, ipc + ht_off[i] + 8);
		writel(0, ipc + ht_off[i] + 12);
	}
	writel(CPIF_MEM_IPC_MAGIC, ipc);
	writel(1, ipc + CPIF_IPC_MEM_ACCESS_OFF);
	if (readl(ipc) != CPIF_MEM_IPC_MAGIC ||
	    readl(ipc + CPIF_IPC_MEM_ACCESS_OFF) != 1) {
		dev_err(c->dev, "legacy link magic readback failed\n");
		return -EACCES;
	}

	/* DPRAM boot magic: while BOOTING the CP expects "BOOT" here;
	 * cmd_phone_start_handler rewrites it to MEM_IPC_MAGIC later. */
	writel(CPIF_MEM_BOOT_MAGIC, ipc);
	return 0;
}

int cpif_legacy_init_now(struct cpif_dev *c)
{
	return cpif_legacy_link_init(c);
}

int cpif_legacy_start_ipc(struct cpif_dev *c)
{
	int ret = cpif_legacy_link_init(c);

	if (!ret) {
		writel(CPIF_MEM_IPC_MAGIC, c->shmem[CPIF_SHMEM_IPC].map);
		if (readl(c->shmem[CPIF_SHMEM_IPC].map) != CPIF_MEM_IPC_MAGIC)
			ret = -EIO;
	}
	return ret;
}

/* A21s vendor DT: legacy RAW queues, boot channel 215, aligned SIPC5. */
#define CPIF_BOOT_TX_OFF		0x3000
#define CPIF_BOOT_TX_SIZE	0x1fd000
#define CPIF_BOOT_RX_OFF		0x200000
#define CPIF_BOOT_RX_SIZE	0x200000
#define CPIF_BOOT_CHANNEL	215
#define CPIF_BOOT_FRAME_SIZE	8

static int cpif_boot_control_ready(struct cpif_dev *c)
{
	struct cpif_shmem_region *r = &c->shmem[CPIF_SHMEM_IPC];

	if (!c->mbox || !r->map ||
	    r->size < CPIF_BOOT_RX_OFF + CPIF_BOOT_RX_SIZE)
		return -ENODEV;
	if (readl(r->map) != CPIF_MEM_BOOT_MAGIC ||
	    !cpif_mbox_extract(c, CPIF_MBX_CP2AP_UNITED,
			       SBI_CP_STATUS_MASK, SBI_CP_STATUS_POS))
		return -EHOSTDOWN;
	return 0;
}

static ssize_t cpif_boot_write(struct file *filp, const char __user *buf,
			      size_t count, loff_t *ppos)
{
	struct cpif_dev *c = cpif_dev_get();
	u8 frame[CPIF_BOOT_FRAME_SIZE] = { 0xfc, CPIF_BOOT_CHANNEL, 8, 0 };
	void __iomem *ipc;
	u32 head, tail, space, command;
	size_t first;
	int ret;

	if (!count)
		return 0;
	if (!c)
		return -ENODEV;
	if (count != sizeof(u32))
		return -EMSGSIZE;
	if (copy_from_user(frame + 4, buf, sizeof(u32)))
		return -EFAULT;
	command = get_unaligned_le32(frame + 4);
	if (command != 0x900d && command != 0x9f00)
		return -EOPNOTSUPP;

	mutex_lock(&c->lock);
	ret = cpif_boot_control_ready(c);
	if (ret)
		goto out;
	ipc = c->shmem[CPIF_SHMEM_IPC].map;
	head = readl(ipc + CPIF_IPC_RAW_HT_OFF);
	tail = readl(ipc + CPIF_IPC_RAW_HT_OFF + 4);
	if (head >= CPIF_BOOT_TX_SIZE || tail >= CPIF_BOOT_TX_SIZE) {
		ret = -EIO;
		goto out;
	}
	space = (tail + CPIF_BOOT_TX_SIZE - head - 1) % CPIF_BOOT_TX_SIZE;
	if (space < sizeof(frame)) {
		ret = -EAGAIN;
		goto out;
	}
	first = min_t(size_t, sizeof(frame), CPIF_BOOT_TX_SIZE - head);
	memcpy_toio(ipc + CPIF_BOOT_TX_OFF + head, frame, first);
	memcpy_toio(ipc + CPIF_BOOT_TX_OFF, frame + first, sizeof(frame) - first);
	wmb();
	writel((head + sizeof(frame)) % CPIF_BOOT_TX_SIZE,
	       ipc + CPIF_IPC_RAW_HT_OFF);
	cpif_mbox_set(c, CPIF_MBX_AP2CP_MSG, 0x83); /* valid | SEND_FMT | SEND_RAW */
	cpif_mbox_raise(c, CPIF_INT_AP2CP_MSG);
	dev_info(c->dev, "UDL control TX 0x%04x\n", command);
	ret = count;
out:
	mutex_unlock(&c->lock);
	return ret;
}

static ssize_t cpif_boot_read(struct file *filp, char __user *buf,
			     size_t count, loff_t *ppos)
{
	struct cpif_dev *c = cpif_dev_get();
	u8 frame[CPIF_BOOT_FRAME_SIZE];
	void __iomem *ipc;
	u32 head, tail, used;
	size_t first;
	int ret;

	if (!count)
		return 0;
	if (!c)
		return -ENODEV;
	if (count < sizeof(u32))
		return -EMSGSIZE;

	mutex_lock(&c->lock);
	ret = cpif_boot_control_ready(c);
	if (ret)
		goto out;
	ipc = c->shmem[CPIF_SHMEM_IPC].map;
	head = readl(ipc + CPIF_IPC_RAW_HT_OFF + 8);
	tail = readl(ipc + CPIF_IPC_RAW_HT_OFF + 12);
	if (head >= CPIF_BOOT_RX_SIZE || tail >= CPIF_BOOT_RX_SIZE) {
		ret = -EIO;
		goto out;
	}
	used = (head + CPIF_BOOT_RX_SIZE - tail) % CPIF_BOOT_RX_SIZE;
	if (used < sizeof(frame)) {
		/* Match the vendor's bounded empty-read behavior; nonblocking
		 * loaders can poll the queue without relying on a lost doorbell.
		 */
		ret = filp->f_flags & O_NONBLOCK ? -EAGAIN : 0;
		goto out;
	}
	rmb();
	first = min_t(size_t, sizeof(frame), CPIF_BOOT_RX_SIZE - tail);
	memcpy_fromio(frame, ipc + CPIF_BOOT_RX_OFF + tail, first);
	memcpy_fromio(frame + first, ipc + CPIF_BOOT_RX_OFF, sizeof(frame) - first);
	if ((frame[0] != 0xf8 && frame[0] != 0xfc) ||
	    frame[1] != CPIF_BOOT_CHANNEL ||
	    get_unaligned_le16(frame + 2) != sizeof(frame)) {
		dev_err(c->dev, "UDL unexpected header %*ph\n", 4, frame);
		ret = -EPROTO;
		goto out;
	}
	if (copy_to_user(buf, frame + 4, sizeof(u32))) {
		ret = -EFAULT;
		goto out;
	}
	wmb();
	writel((tail + sizeof(frame)) % CPIF_BOOT_RX_SIZE,
	       ipc + CPIF_IPC_RAW_HT_OFF + 12);
	dev_info(c->dev, "UDL control RX 0x%04x\n", get_unaligned_le32(frame + 4));
	ret = sizeof(u32);
out:
	mutex_unlock(&c->lock);
	return ret;
}

static int cpif_security_req(struct cpif_dev *c, struct cpif_sec_req *msr)
{
	unsigned long mode = msr->mode, ret;
	unsigned int tries = 0;

	/* The CP may have overwritten the TOC area in cp_rmem while it was
	 * running; re-parse so mem_map_on_cp reflects the image just
	 * uploaded by LOAD_CP_IMAGE. */
	cpif_dump_toc(c);

	if (msr->mode == CPIF_BOOT_MODE_NORMAL && c->mem_map_on_cp)
		mode |= (u32)c->shmem[CPIF_SHMEM_CP].phys;

	cpif_smc(CPIF_SMC_ID_CLK, CPIF_SSS_CLK_ENABLE, 0, 0);
	dev_info(c->dev, "REQ_SECURITY smc mode 0x%lx p2 0x%x p3 0x%x\n",
		 mode, msr->param2, msr->param3);
	ret = cpif_smc(CPIF_SMC_ID, mode, msr->param2, msr->param3);
	while (ret == CPIF_SMC_SIGN_NOT_FINISH && tries < CPIF_SMC_MAX_TRY) {
		tries++;
		ret = cpif_smc(CPIF_SMC_ID, CPIF_SMC_MODE_CHECK_CONT, 0, 0);
	}
	cpif_smc(CPIF_SMC_ID_CLK, CPIF_SSS_CLK_DISABLE, 0, 0);

	dev_info(c->dev, "REQ_SECURITY result 0x%lx (%u polls)\n", ret, tries);
	return ret ? -EIO : 0;
}

static int cpif_start_normal_boot(struct cpif_dev *c)
{
	int ds = c->ds_detect ? c->ds_detect : 2;
	void __iomem *ipc = c->shmem[CPIF_SHMEM_IPC].map;
	int cnt = 200;
	int ret;
	u32 v;

	if (!c->mbox)
		return -ENODEV;

	cpif_mbox_set(c, CPIF_MBX_AP2CP_MSG, 0);
	cpif_mbox_set(c, CPIF_MBX_CP2AP_MSG, 0);
	cpif_mbox_set(c, CPIF_MBX_AP2CP_UNITED, 0);
	cpif_mbox_set(c, CPIF_MBX_CP2AP_UNITED, 0);
	cpif_mbox_update(c, CPIF_MBX_AP2CP_UNITED, ds - 1, SBI_DS_DET_MASK, SBI_DS_DET_POS);
	cpif_mbox_update(c, CPIF_MBX_AP2CP_UNITED, 0, SBI_SYS_REV_MASK, SBI_SYS_REV_POS);

	c->phone_started = false;
	c->sbd_ready = false;
	c->cp2ap_events = 0;
	reinit_completion(&c->init_cmpl);

	if (ipc) {
		ret = cpif_legacy_link_init(c);
		if (ret)
			return ret;
	}
	c->phone_state = CPIF_STATE_BOOTING;

	if (c->ap2cp_cfg) {
		writel(1, c->ap2cp_cfg);
		if (readl(c->ap2cp_cfg) != 1) {
			dev_err(c->dev, "AP2CP_CFG readback != 1\n");
			return -EIO;
		}
	}

	while (!cpif_mbox_extract(c, CPIF_MBX_CP2AP_UNITED, SBI_CP_STATUS_MASK, SBI_CP_STATUS_POS)) {
		if (--cnt <= 0) {
			cpif_mbox_dump(c, "cp_status timeout");
			return -EACCES;
		}
		usleep_range(10000, 20000);
	}

	v = cpif_mbox_extract(c, CPIF_MBX_CP2AP_UNITED, SBI_CP_STATUS_MASK, SBI_CP_STATUS_POS);
	dev_info(c->dev, "cp_status=%u after %d polls\n", v, 200 - cnt);

	cpif_mbox_update(c, CPIF_MBX_AP2CP_UNITED, 1, SBI_AP_STATUS_MASK, SBI_AP_STATUS_POS);
	cpif_mbox_update(c, CPIF_MBX_AP2CP_UNITED, 1, SBI_PDA_ACTIVE_MASK, SBI_PDA_ACTIVE_POS);
	cpif_mbox_dump(c, "boot started");
	return 0;
}

static int cpif_complete_normal_boot(struct cpif_dev *c)
{
	if (!wait_for_completion_timeout(&c->init_cmpl, CPIF_INIT_TIMEOUT)) {
		cpif_mbox_dump(c, "complete timeout");
		return -EAGAIN;
	}
	mutex_lock(&c->lock);
	c->phone_state = CPIF_STATE_ONLINE;
	mutex_unlock(&c->lock);
	return 0;
}

static long cpif_boot_ioctl(struct file *filp, unsigned int cmd,
			    unsigned long arg)
{
	struct cpif_dev *c = cpif_dev_get();
	int ret = 0;

	if (!c)
		return -ENODEV;

	mutex_lock(&c->lock);

	switch (cmd) {
	case CPIF_IOC_POWER_ON:
		ret = cpif_power_on(c);
		break;

	case CPIF_IOC_POWER_OFF:
		ret = cpif_power_off(c);
		break;

	case CPIF_IOC_POWER_RESET:
	case CPIF_IOC_START_CP_BOOTLOADER:
	{
		struct cpif_boot_mode mode;

		if (copy_from_user(&mode, (void __user *)arg, sizeof(mode))) {
			ret = -EFAULT;
			break;
		}
		if (mode.idx > CPIF_BOOT_MODE_DUMP) {
			ret = -EINVAL;
			break;
		}
		if (cmd == CPIF_IOC_POWER_RESET)
			ret = cpif_power_reset(c, mode.idx);
		else if (mode.idx == CPIF_BOOT_MODE_NORMAL)
			ret = cpif_start_normal_boot(c);
		else
			ret = -EOPNOTSUPP;
		break;
	}

	case CPIF_IOC_LOAD_CP_IMAGE:
	{
		struct cpif_image img;
		void __iomem *dst;
		void __user *src;
		void *kbuf;
		u32 done = 0;

		if (copy_from_user(&img, (void __user *)arg, sizeof(img))) {
			ret = -EFAULT;
			break;
		}
		src = (void __user *)(unsigned long)img.binary;

		if (!img.len || img.len > c->cp_size ||
		    img.m_offset > c->cp_size - img.len) {
			ret = -EINVAL;
			break;
		}

		dst = cpif_shmem_resolve(c, img.m_offset, img.len);
		if (!dst) {
			dev_err(c->dev,
				"cp image: off 0x%x len 0x%x outside mapped shmem\n",
				img.m_offset, img.len);
			ret = -EINVAL;
			break;
		}

		kbuf = kmalloc(min_t(u32, img.len, 65536), GFP_KERNEL);
		if (!kbuf) {
			ret = -ENOMEM;
			break;
		}

		while (done < img.len) {
			u32 chunk = min_t(u32, img.len - done, 65536);

			if (copy_from_user(kbuf, src + done, chunk)) {
				ret = -EFAULT;
				break;
			}
			memcpy_toio(dst + done, kbuf, chunk);
			done += chunk;
		}
		kfree(kbuf);
		if (ret)
			break;

		dev_info_ratelimited(c->dev,
				     "cp image: off 0x%x len 0x%x mode %u\n",
				     img.m_offset, img.len, img.mode);
		break;
	}

	case CPIF_IOC_REQ_SECURITY:
	{
		struct cpif_sec_req msr;

		if (copy_from_user(&msr, (void __user *)arg, sizeof(msr))) {
			ret = -EFAULT;
			break;
		}
		ret = cpif_security_req(c, &msr);
		break;
	}

	case CPIF_IOC_COMPLETE_NORMAL_BOOTUP:
		/* The threaded mailbox handler needs the lock to initialise IPC. */
		mutex_unlock(&c->lock);
		return cpif_complete_normal_boot(c);

	case CPIF_IOC_GET_CP_STATUS:
		ret = c->phone_state;
		break;

	case CPIF_IOC_SET_CP_STATUS:
	{
		__u32 st;

		if (copy_from_user(&st, (void __user *)arg, sizeof(st))) {
			ret = -EFAULT;
			break;
		}
		if (st >= CPIF_STATE_MAX) {
			ret = -EINVAL;
			break;
		}
		c->phone_state = (enum cpif_modem_state)st;
		cpif_dump_toc(c);
		wake_up_interruptible(&c->wq);
		dev_info(c->dev, "CP state updated to %u (toc: %s)\n",
			 st, c->toc_summary[0] ? c->toc_summary : "none");
		ret = 0;
		break;
	}

	case CPIF_IOC_START_CP_DUMP:
	case CPIF_IOC_TRIGGER_CP_CRASH:
	case CPIF_IOC_TRIGGER_KERNEL_PANIC:
		/* TODO: crash/dump paths. */
		ret = -EOPNOTSUPP;
		break;

	default:
		ret = -ENOIOCTLCMD;
		break;
	}

	mutex_unlock(&c->lock);
	return ret;
}

static const struct file_operations cpif_boot_fops = {
	.owner		= THIS_MODULE,
	.read		= cpif_boot_read,
	.write		= cpif_boot_write,
	.unlocked_ioctl	= cpif_boot_ioctl,
	.compat_ioctl	= cpif_boot_ioctl,
	.llseek		= noop_llseek,
};

int cpif_boot_register(struct cpif_dev *c)
{
	c->boot_dev.minor = MISC_DYNAMIC_MINOR;
	c->boot_dev.name = CPIF_BOOT_DEV_NAME;
	c->boot_dev.fops = &cpif_boot_fops;
	c->boot_dev.parent = c->dev;

	return misc_register(&c->boot_dev);
}

void cpif_boot_unregister(struct cpif_dev *c)
{
	misc_deregister(&c->boot_dev);
}
