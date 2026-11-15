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
 * TODO before this works on hardware:
 *  - CPIF_IOC_REQ_SECURITY: needs the SMC id pair used by vendor
 *    shmem_security_request() (SMC_ID / SMC_ID_CLK under
 *    CONFIG_CP_SECURE_BOOT). Grep vendor modem_prj.h / linux/smc.h
 *    for the exynos850 values and add an exynos_smc() wrapper.
 *  - CPIF_IOC_START_CP_BOOTLOADER: vendor also pokes mbox interrupts
 *    (mbox_set_interrupt, mbox_sw_reset) and waits for CP "boot done"
 *    via mailbox/IPC status. Implement once cp_mailbox driver lands.
 *  - CP image layout: vendor expects bootloader image at low offset
 *    and main binary via m_offset; cbd passes raw chunks. Our copy
 *    honours img.m_offset into the CP region directly.
 */
#include <linux/fs.h>
#include <linux/miscdevice.h>
#include <linux/uaccess.h>

#include "cpif.h"

#define CPIF_BOOT_DEV_NAME	"umts_boot0"

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
		if (mode.idx > CPIF_BOOT_MODE_MANUAL) {
			ret = -EINVAL;
			break;
		}
		/* START_CP_BOOTLOADER shares the reset path in vendor;
		 * NORMAL -> power_reset, DUMP -> power_reset_dump. */
		ret = cpif_power_reset(c, mode.idx);
		break;
	}

	case CPIF_IOC_LOAD_CP_IMAGE:
	{
		struct cpif_image img;
		void __iomem *dst;
		void __user *src;

		if (copy_from_user(&img, (void __user *)arg, sizeof(img))) {
			ret = -EFAULT;
			break;
		}
		src = (void __user *)(unsigned long)img.binary;

		/* vendor: valid_space = mode ? mld->size : mld->boot_size;
		 * mode 0 = boot area, non-0 = main/dump area. Our CP region
		 * is one contiguous block; treat m_offset as absolute
		 * offset into it (matches vendor for mode 0). */
		if (img.len > c->cp_size ||
		    img.m_offset > c->cp_size - img.len) {
			ret = -EINVAL;
			break;
		}

		dst = c->shmem[CPIF_SHMEM_CP].map + img.m_offset;
		if (!dst) {
			ret = -ENODEV;
			break;
		}

		if (copy_from_user(dst, src, img.len)) {
			ret = -EFAULT;
			break;
		}
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
		/* TODO: call secure monitor to authenticate CP image.
		 * Vendor shmem_security_request() -> exynos_smc(SMC_ID,
		 * mode, param2, param3). On non-secure-boot config it
		 * only CRC32-checks the image; we do the same TODO. */
		dev_info(c->dev,
			 "REQ_SECURITY mode 0x%x p2 0x%x p3 0x%x - not impl\n",
			 msr.mode, msr.param2, msr.param3);
		ret = -EOPNOTSUPP;
		break;
	}

	case CPIF_IOC_COMPLETE_NORMAL_BOOTUP:
		/* TODO: wait for CP boot-done indication (mailbox/IPC). */
		dev_info(c->dev, "COMPLETE_NORMAL_BOOTUP - not impl\n");
		ret = -EOPNOTSUPP;
		break;

	case CPIF_IOC_GET_CP_STATUS:
		ret = c->phone_state;
		break;

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
