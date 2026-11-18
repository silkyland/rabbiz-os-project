// SPDX-License-Identifier: GPL-2.0
/*
 * /dev/umts_ipc0, /dev/umts_ipc1, /dev/umts_rfs0 - SIPC data channel
 * stubs for the A21s cpif skeleton.
 *
 * Vendor implementation lives in link_device_memory*.c +
 * link_rx_pktproc.c + mcu_ipc.c: per-channel TX/RX ring buffers in
 * the "IPC" shmem region, doorbell interrupts through cp_mailbox
 * (0x11920000), and SIPC v5.0 frame formats from include/sipc5.h.
 *
 * This file only creates the device nodes so userspace (libsamsung-ipc)
 * can open them; every data op returns -EIO until the shmem queue +
 * mailbox engine is implemented.
 */
#include <linux/fs.h>
#include <linux/miscdevice.h>
#include <linux/uaccess.h>

#include "cpif.h"

#define CPIF_IPC_DEV_NAMES	{ "umts_ipc0", "umts_ipc1" }
#define CPIF_RFS_DEV_NAME	"umts_rfs0"

static int cpif_ipc_open(struct inode *inode, struct file *filp)
{
	struct miscdevice *m = filp->private_data;
	struct cpif_dev *c = dev_get_drvdata(m->parent);
	atomic_t *opened;

	if (!c)
		return -ENODEV;
	opened = (m == &c->rfs_dev) ? &c->rfs_opened : &c->fmt_opened;
	atomic_inc(opened);
	filp->private_data = opened;
	return 0;
}

static int cpif_ipc_release(struct inode *inode, struct file *filp)
{
	atomic_dec((atomic_t *)filp->private_data);
	return 0;
}

bool cpif_rild_ready(struct cpif_dev *c)
{
	return atomic_read(&c->fmt_opened) > 0 &&
	       atomic_read(&c->rfs_opened) > 0;
}

static ssize_t cpif_ipc_read(struct file *filp, char __user *buf,
			     size_t count, loff_t *ppos)
{
	/* TODO: dequeue SIPC frames from RX ring, wake on mailbox IRQ */
	return -EIO;
}

static ssize_t cpif_ipc_write(struct file *filp, const char __user *buf,
			      size_t count, loff_t *ppos)
{
	/* TODO: enqueue SIPC frames to TX ring + ring mailbox doorbell */
	return -EIO;
}

static long cpif_ipc_ioctl(struct file *filp, unsigned int cmd,
			   unsigned long arg)
{
	/* Vendor ioctls on ipc devices: link status, pdn/cid mgmt,
	 * flow control. TODO when userspace needs them. */
	return -ENOIOCTLCMD;
}

static const struct file_operations cpif_ipc_fops = {
	.owner		= THIS_MODULE,
	.open		= cpif_ipc_open,
	.release	= cpif_ipc_release,
	.read		= cpif_ipc_read,
	.write		= cpif_ipc_write,
	.unlocked_ioctl	= cpif_ipc_ioctl,
	.compat_ioctl	= cpif_ipc_ioctl,
	.llseek		= noop_llseek,
};

int cpif_ipc_register(struct cpif_dev *c)
{
	static const char * const names[2] = CPIF_IPC_DEV_NAMES;
	int i, ret;

	for (i = 0; i < 2; i++) {
		c->ipc_dev[i].minor = MISC_DYNAMIC_MINOR;
		c->ipc_dev[i].name = names[i];
		c->ipc_dev[i].fops = &cpif_ipc_fops;
		c->ipc_dev[i].parent = c->dev;

		ret = misc_register(&c->ipc_dev[i]);
		if (ret) {
			dev_err(c->dev, "register %s failed: %d\n",
				names[i], ret);
			while (--i >= 0)
				misc_deregister(&c->ipc_dev[i]);
			return ret;
		}
	}

	c->rfs_dev.minor = MISC_DYNAMIC_MINOR;
	c->rfs_dev.name = CPIF_RFS_DEV_NAME;
	c->rfs_dev.fops = &cpif_ipc_fops;
	c->rfs_dev.parent = c->dev;

	ret = misc_register(&c->rfs_dev);
	if (ret) {
		dev_err(c->dev, "register %s failed: %d\n",
			CPIF_RFS_DEV_NAME, ret);
		cpif_ipc_unregister(c);
	}
	return ret;
}

void cpif_ipc_unregister(struct cpif_dev *c)
{
	misc_deregister(&c->rfs_dev);
	misc_deregister(&c->ipc_dev[0]);
	misc_deregister(&c->ipc_dev[1]);
}
