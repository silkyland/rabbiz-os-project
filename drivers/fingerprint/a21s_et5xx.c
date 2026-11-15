// SPDX-License-Identifier: GPL-2.0
/*
 * EgisTech ET520 (et5xx) fingerprint sensor driver for Samsung Galaxy A21s
 * (SM-A217F, Exynos 850).
 *
 * Claims the ET520 hardware control GPIOs from the vendor schematic / DTS:
 *   - Sleep / Reset GPIO : gpg1-1 (139b0000.pinctrl)
 *   - DRDY / IRQ GPIO    : gpa0-5 (11850000.pinctrl)
 *
 * Registers /dev/esfp0 (poll/read/write/ioctl) and /sys/kernel/a21s_fp/
 * so userspace shells can receive hardware DRDY edges or trigger biometric
 * unlock events cleanly without reflashing the DTB.
 */
#include <linux/atomic.h>
#include <linux/delay.h>
#include <linux/fs.h>
#include <linux/gpio/consumer.h>
#include <linux/gpio/driver.h>
#include <linux/gpio/machine.h>
#include <linux/interrupt.h>
#include <linux/jiffies.h>
#include <linux/kobject.h>
#include <linux/miscdevice.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/poll.h>
#include <linux/slab.h>
#include <linux/sysfs.h>
#include <linux/uaccess.h>
#include <linux/wait.h>

#define ET5XX_DEV_NAME		"esfp0"
#define ET5XX_SLEEP_CHIP	"gpg1"
#define ET5XX_SLEEP_LINE	1
#define ET5XX_DRDY_CHIP		"gpa0"
#define ET5XX_DRDY_LINE		5

/* Vendor ET5xx ioctl magic & commands (subset) */
#define ET5XX_IOC_MAGIC		'k'
#define ET5XX_IOC_RESET		_IO(ET5XX_IOC_MAGIC, 0x01)
#define ET5XX_IOC_GET_IRQ_CNT	_IOR(ET5XX_IOC_MAGIC, 0x02, unsigned int)

struct a21s_fp_dev {
	struct gpio_device	*sleep_gdev;
	struct gpio_device	*drdy_gdev;
	struct gpio_desc	*sleep_desc;
	struct gpio_desc	*drdy_desc;
	int			irq;
	bool			irq_enabled;
	bool			flood_guarded;
	int			sleep_val;

	atomic_t		irq_count;
	unsigned long		last_event_jiffies;
	unsigned long		burst_window_start;
	int			burst_count;

	wait_queue_head_t	wq;
	struct mutex		lock;
	struct miscdevice	mdev;
	struct kobject		*kobj;
};

static struct a21s_fp_dev *g_fp;

static void et5xx_pulse_reset(struct a21s_fp_dev *fp)
{
	if (!fp->sleep_desc)
		return;

	mutex_lock(&fp->lock);
	gpiod_set_value_cansleep(fp->sleep_desc, 0);
	msleep(5);
	gpiod_set_value_cansleep(fp->sleep_desc, 1);
	fp->sleep_val = 1;
	mutex_unlock(&fp->lock);
	msleep(2);
}

static void et5xx_record_event(struct a21s_fp_dev *fp)
{
	atomic_inc(&fp->irq_count);
	fp->last_event_jiffies = jiffies;
	wake_up_interruptible(&fp->wq);
}

static irqreturn_t et5xx_irq_thread(int irq, void *data)
{
	struct a21s_fp_dev *fp = data;
	unsigned long now = jiffies;

	mutex_lock(&fp->lock);
	/* Guard against floating DRDY pin storms when VDD_BTP is off */
	if (time_after(now, fp->burst_window_start + msecs_to_jiffies(200))) {
		fp->burst_window_start = now;
		fp->burst_count = 1;
	} else {
		fp->burst_count++;
		if (fp->burst_count > 6 && fp->irq_enabled) {
			disable_irq_nosync(fp->irq);
			fp->irq_enabled = false;
			fp->flood_guarded = true;
			mutex_unlock(&fp->lock);
			pr_warn("a21s-et5xx: floating DRDY storm suppressed (irq %d)\n",
				fp->irq);
			return IRQ_HANDLED;
		}
	}

	/* Debounce real finger-touch edges at 250 ms */
	if (time_before(now, fp->last_event_jiffies + msecs_to_jiffies(250))) {
		mutex_unlock(&fp->lock);
		return IRQ_HANDLED;
	}
	mutex_unlock(&fp->lock);

	et5xx_record_event(fp);
	return IRQ_HANDLED;
}

/* ---- /dev/esfp0 file_operations ----------------------------------- */

struct esfp_file_ctx {
	int	seen_cnt;
};

static int esfp_open(struct inode *inode, struct file *filp)
{
	struct esfp_file_ctx *ctx;

	if (!g_fp)
		return -ENODEV;

	ctx = kzalloc(sizeof(*ctx), GFP_KERNEL);
	if (!ctx)
		return -ENOMEM;

	ctx->seen_cnt = atomic_read(&g_fp->irq_count);
	filp->private_data = ctx;
	return 0;
}

static int esfp_release(struct inode *inode, struct file *filp)
{
	kfree(filp->private_data);
	return 0;
}

static __poll_t esfp_poll(struct file *filp, poll_table *wait)
{
	struct esfp_file_ctx *ctx = filp->private_data;
	__poll_t mask = 0;

	poll_wait(filp, &g_fp->wq, wait);
	if (atomic_read(&g_fp->irq_count) != ctx->seen_cnt)
		mask |= EPOLLIN | EPOLLRDNORM;
	return mask;
}

static ssize_t esfp_read(struct file *filp, char __user *buf,
			 size_t count, loff_t *ppos)
{
	struct esfp_file_ctx *ctx = filp->private_data;
	char msg[48];
	int cur, len;

	if (filp->f_flags & O_NONBLOCK) {
		cur = atomic_read(&g_fp->irq_count);
		if (cur == ctx->seen_cnt)
			return -EAGAIN;
	} else {
		int ret = wait_event_interruptible(
			g_fp->wq, atomic_read(&g_fp->irq_count) != ctx->seen_cnt);
		if (ret)
			return ret;
		cur = atomic_read(&g_fp->irq_count);
	}

	ctx->seen_cnt = cur;
	len = scnprintf(msg, sizeof(msg), "unlock %d\n", cur);
	if ((size_t)len > count)
		len = count;
	if (copy_to_user(buf, msg, len))
		return -EFAULT;
	return len;
}

static ssize_t esfp_write(struct file *filp, const char __user *buf,
			  size_t count, loff_t *ppos)
{
	char cmd[32];
	size_t n = min_t(size_t, count, sizeof(cmd) - 1);

	if (copy_from_user(cmd, buf, n))
		return -EFAULT;
	cmd[n] = '\0';

	if (!strncmp(cmd, "reset", 5))
		et5xx_pulse_reset(g_fp);
	else
		et5xx_record_event(g_fp);

	return count;
}

static long esfp_ioctl(struct file *filp, unsigned int cmd, unsigned long arg)
{
	unsigned int cnt;

	switch (cmd) {
	case ET5XX_IOC_RESET:
		et5xx_pulse_reset(g_fp);
		return 0;
	case ET5XX_IOC_GET_IRQ_CNT:
		cnt = (unsigned int)atomic_read(&g_fp->irq_count);
		if (copy_to_user((void __user *)arg, &cnt, sizeof(cnt)))
			return -EFAULT;
		return 0;
	default:
		return -ENOIOCTLCMD;
	}
}

static const struct file_operations esfp_fops = {
	.owner		= THIS_MODULE,
	.open		= esfp_open,
	.release	= esfp_release,
	.poll		= esfp_poll,
	.read		= esfp_read,
	.write		= esfp_write,
	.unlocked_ioctl	= esfp_ioctl,
	.llseek		= noop_llseek,
};

/* ---- /sys/kernel/a21s_fp/ attributes ------------------------------ */

static ssize_t chipid_show(struct kobject *kobj, struct kobj_attribute *attr,
			   char *buf)
{
	return sysfs_emit(buf, "ET520\n");
}
static struct kobj_attribute attr_chipid = __ATTR_RO(chipid);

static ssize_t status_show(struct kobject *kobj, struct kobj_attribute *attr,
			   char *buf)
{
	if (!g_fp)
		return sysfs_emit(buf, "offline\n");
	if (g_fp->flood_guarded)
		return sysfs_emit(buf, "ready-sw-irq\n");
	return sysfs_emit(buf, "ready\n");
}
static struct kobj_attribute attr_status = __ATTR_RO(status);

static ssize_t irq_count_show(struct kobject *kobj,
			      struct kobj_attribute *attr, char *buf)
{
	return sysfs_emit(buf, "%d\n", atomic_read(&g_fp->irq_count));
}
static struct kobj_attribute attr_irq_count = __ATTR_RO(irq_count);

static ssize_t drdy_level_show(struct kobject *kobj,
			       struct kobj_attribute *attr, char *buf)
{
	int val = g_fp->drdy_desc ?
		gpiod_get_value_cansleep(g_fp->drdy_desc) : -ENODEV;

	return sysfs_emit(buf, "%d\n", val);
}
static struct kobj_attribute attr_drdy_level = __ATTR_RO(drdy_level);

static ssize_t sleep_pin_show(struct kobject *kobj,
			      struct kobj_attribute *attr, char *buf)
{
	return sysfs_emit(buf, "%d\n", g_fp->sleep_val);
}

static ssize_t sleep_pin_store(struct kobject *kobj,
			       struct kobj_attribute *attr,
			       const char *buf, size_t count)
{
	unsigned int val;

	if (kstrtouint(buf, 0, &val))
		return -EINVAL;

	if (val == 2) {
		et5xx_pulse_reset(g_fp);
	} else {
		mutex_lock(&g_fp->lock);
		if (g_fp->sleep_desc) {
			gpiod_set_value_cansleep(g_fp->sleep_desc, !!val);
			g_fp->sleep_val = !!val;
		}
		mutex_unlock(&g_fp->lock);
	}
	return count;
}
static struct kobj_attribute attr_sleep_pin = __ATTR_RW(sleep_pin);

static ssize_t trigger_unlock_show(struct kobject *kobj,
				   struct kobj_attribute *attr, char *buf)
{
	return sysfs_emit(buf, "%d\n", atomic_read(&g_fp->irq_count));
}

static ssize_t trigger_unlock_store(struct kobject *kobj,
				    struct kobj_attribute *attr,
				    const char *buf, size_t count)
{
	unsigned int val = 1;

	if (kstrtouint(buf, 0, &val) && buf[0] != '\n')
		return -EINVAL;
	if (val)
		et5xx_record_event(g_fp);
	return count;
}
static struct kobj_attribute attr_trigger_unlock = __ATTR_RW(trigger_unlock);

static struct attribute *fp_attrs[] = {
	&attr_chipid.attr,
	&attr_status.attr,
	&attr_irq_count.attr,
	&attr_drdy_level.attr,
	&attr_sleep_pin.attr,
	&attr_trigger_unlock.attr,
	NULL,
};

static const struct attribute_group fp_attr_group = {
	.attrs = fp_attrs,
};

static int __init a21s_et5xx_init(void)
{
	struct a21s_fp_dev *fp;
	struct gpio_chip *sleep_gc, *drdy_gc;
	int ret;

	fp = kzalloc(sizeof(*fp), GFP_KERNEL);
	if (!fp)
		return -ENOMEM;

	mutex_init(&fp->lock);
	init_waitqueue_head(&fp->wq);
	atomic_set(&fp->irq_count, 0);
	fp->last_event_jiffies = jiffies - msecs_to_jiffies(1000);
	fp->burst_window_start = jiffies;

	fp->sleep_gdev = gpio_device_find_by_label(ET5XX_SLEEP_CHIP);
	if (!fp->sleep_gdev) {
		pr_err("a21s-et5xx: gpiochip %s not found\n", ET5XX_SLEEP_CHIP);
		ret = -ENODEV;
		goto err_free;
	}
	sleep_gc = gpio_device_get_chip(fp->sleep_gdev);
	fp->sleep_desc = gpiochip_request_own_desc(sleep_gc, ET5XX_SLEEP_LINE,
						   "et5xx_sleep",
						   GPIO_ACTIVE_HIGH,
						   GPIOD_OUT_HIGH);
	if (IS_ERR(fp->sleep_desc)) {
		ret = PTR_ERR(fp->sleep_desc);
		fp->sleep_desc = NULL;
		pr_err("a21s-et5xx: request %s-%u failed: %d\n",
		       ET5XX_SLEEP_CHIP, ET5XX_SLEEP_LINE, ret);
		goto err_sleep_gdev;
	}
	fp->sleep_val = 1;

	fp->drdy_gdev = gpio_device_find_by_label(ET5XX_DRDY_CHIP);
	if (!fp->drdy_gdev) {
		pr_err("a21s-et5xx: gpiochip %s not found\n", ET5XX_DRDY_CHIP);
		ret = -ENODEV;
		goto err_sleep_desc;
	}
	drdy_gc = gpio_device_get_chip(fp->drdy_gdev);
	fp->drdy_desc = gpiochip_request_own_desc(drdy_gc, ET5XX_DRDY_LINE,
						  "et5xx_drdy",
						  GPIO_ACTIVE_HIGH,
						  GPIOD_IN);
	if (IS_ERR(fp->drdy_desc)) {
		ret = PTR_ERR(fp->drdy_desc);
		fp->drdy_desc = NULL;
		pr_err("a21s-et5xx: request %s-%u failed: %d\n",
		       ET5XX_DRDY_CHIP, ET5XX_DRDY_LINE, ret);
		goto err_drdy_gdev;
	}

	g_fp = fp;
	et5xx_pulse_reset(fp);

	fp->irq = gpiod_to_irq(fp->drdy_desc);
	if (fp->irq > 0) {
		ret = request_threaded_irq(fp->irq, NULL, et5xx_irq_thread,
					   IRQF_TRIGGER_RISING | IRQF_ONESHOT,
					   "et5xx_drdy", fp);
		if (ret) {
			pr_warn("a21s-et5xx: request_irq(%d) failed: %d\n",
				fp->irq, ret);
			fp->irq = 0;
		} else {
			fp->irq_enabled = true;
		}
	}

	fp->mdev.minor = MISC_DYNAMIC_MINOR;
	fp->mdev.name = ET5XX_DEV_NAME;
	fp->mdev.fops = &esfp_fops;
	ret = misc_register(&fp->mdev);
	if (ret) {
		pr_err("a21s-et5xx: misc_register /dev/%s failed: %d\n",
		       ET5XX_DEV_NAME, ret);
		goto err_irq;
	}

	fp->kobj = kobject_create_and_add("a21s_fp", kernel_kobj);
	if (fp->kobj) {
		ret = sysfs_create_group(fp->kobj, &fp_attr_group);
		if (ret)
			pr_warn("a21s-et5xx: sysfs_create_group failed: %d\n",
				ret);
	}

	pr_info("a21s-et5xx: EgisTech ET520 ready (/dev/%s, sleep=%s-%u, drdy=%s-%u irq=%d)\n",
		ET5XX_DEV_NAME, ET5XX_SLEEP_CHIP, ET5XX_SLEEP_LINE,
		ET5XX_DRDY_CHIP, ET5XX_DRDY_LINE, fp->irq);
	return 0;

err_irq:
	if (fp->irq > 0)
		free_irq(fp->irq, fp);
	g_fp = NULL;
	gpiochip_free_own_desc(fp->drdy_desc);
err_drdy_gdev:
	gpio_device_put(fp->drdy_gdev);
err_sleep_desc:
	gpiochip_free_own_desc(fp->sleep_desc);
err_sleep_gdev:
	gpio_device_put(fp->sleep_gdev);
err_free:
	kfree(fp);
	return ret;
}

static void __exit a21s_et5xx_exit(void)
{
	struct a21s_fp_dev *fp = g_fp;

	if (!fp)
		return;

	if (fp->kobj) {
		sysfs_remove_group(fp->kobj, &fp_attr_group);
		kobject_put(fp->kobj);
	}
	misc_deregister(&fp->mdev);
	if (fp->irq > 0)
		free_irq(fp->irq, fp);
	if (fp->sleep_desc) {
		gpiod_set_value_cansleep(fp->sleep_desc, 0);
		gpiochip_free_own_desc(fp->sleep_desc);
	}
	if (fp->drdy_desc)
		gpiochip_free_own_desc(fp->drdy_desc);
	if (fp->drdy_gdev)
		gpio_device_put(fp->drdy_gdev);
	if (fp->sleep_gdev)
		gpio_device_put(fp->sleep_gdev);
	g_fp = NULL;
	kfree(fp);
}

module_init(a21s_et5xx_init);
module_exit(a21s_et5xx_exit);

MODULE_DESCRIPTION("Samsung Galaxy A21s EgisTech ET520 fingerprint sensor driver");
MODULE_LICENSE("GPL");
