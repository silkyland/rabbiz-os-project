// SPDX-License-Identifier: GPL-2.0
/*
 * Samsung Galaxy A21s (SM-A217F) COINDC ERM vibration motor driver.
 *
 * Vendor DTS (a21s_eur_open_w00_r06.dts, s2mu106-haptic) configures:
 *   - haptic,operation_mode = <1> (S2MU106_HAPTIC_ERM_GPIO)
 *   - haptic,motor_en = <&gpg0 0 0>
 *   - haptic,motor_type = "COINDC"
 *
 * Exposes:
 *   - Standard Linux input force-feedback device (EV_FF / FF_RUMBLE)
 *   - LED class device (/sys/class/leds/vibrator/brightness)
 *   - /sys/kernel/a21s_vib/duration_ms (write ms to pulse, 0 to stop)
 */

#include <linux/delay.h>
#include <linux/gpio/consumer.h>
#include <linux/gpio/driver.h>
#include <linux/gpio/machine.h>
#include <linux/input.h>
#include <linux/kobject.h>
#include <linux/leds.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/slab.h>
#include <linux/sysfs.h>
#include <linux/workqueue.h>

#define VIB_GPIO_CHIP		"gpg0"
#define VIB_GPIO_LINE		0
#define VIB_MAX_DURATION_MS	10000

struct a21s_vib_dev {
	struct gpio_device	*gdev;
	struct gpio_desc	*en_desc;
	struct input_dev	*input;
	struct led_classdev	led;
	struct kobject		*kobj;

	struct delayed_work	stop_work;
	struct work_struct	ff_work;
	struct mutex		lock;

	bool			active;
	u16			ff_magnitude;
	unsigned int		last_duration_ms;
};

static struct a21s_vib_dev *g_vib;

static void a21s_vib_set_hw(struct a21s_vib_dev *vib, bool on)
{
	if (!vib->en_desc)
		return;

	gpiod_set_value_cansleep(vib->en_desc, on ? 1 : 0);
	vib->active = on;
}

static void a21s_vib_stop_work(struct work_struct *work)
{
	struct a21s_vib_dev *vib =
		container_of(to_delayed_work(work), struct a21s_vib_dev, stop_work);

	mutex_lock(&vib->lock);
	a21s_vib_set_hw(vib, false);
	mutex_unlock(&vib->lock);
}

static void a21s_vib_trigger(struct a21s_vib_dev *vib, unsigned int duration_ms)
{
	cancel_delayed_work_sync(&vib->stop_work);

	mutex_lock(&vib->lock);
	if (duration_ms == 0) {
		a21s_vib_set_hw(vib, false);
	} else {
		if (duration_ms > VIB_MAX_DURATION_MS)
			duration_ms = VIB_MAX_DURATION_MS;
		vib->last_duration_ms = duration_ms;
		a21s_vib_set_hw(vib, true);
		schedule_delayed_work(&vib->stop_work,
				      msecs_to_jiffies(duration_ms));
	}
	mutex_unlock(&vib->lock);
}

static void a21s_vib_ff_work(struct work_struct *work)
{
	struct a21s_vib_dev *vib =
		container_of(work, struct a21s_vib_dev, ff_work);

	mutex_lock(&vib->lock);
	a21s_vib_set_hw(vib, vib->ff_magnitude > 0);
	mutex_unlock(&vib->lock);
}

static int a21s_vib_play_effect(struct input_dev *dev, void *data,
				struct ff_effect *effect)
{
	struct a21s_vib_dev *vib = input_get_drvdata(dev);
	u32 mag;

	if (effect->type != FF_RUMBLE)
		return 0;

	mag = effect->u.rumble.strong_magnitude;
	if (!mag)
		mag = effect->u.rumble.weak_magnitude;

	vib->ff_magnitude = (u16)mag;
	schedule_work(&vib->ff_work);
	return 0;
}

static int a21s_vib_led_set(struct led_classdev *cdev,
			    enum led_brightness value)
{
	struct a21s_vib_dev *vib = container_of(cdev, struct a21s_vib_dev, led);

	cancel_delayed_work_sync(&vib->stop_work);
	mutex_lock(&vib->lock);
	a21s_vib_set_hw(vib, value != LED_OFF);
	mutex_unlock(&vib->lock);
	return 0;
}

static ssize_t duration_ms_show(struct kobject *kobj,
				struct kobj_attribute *attr, char *buf)
{
	if (!g_vib)
		return -ENODEV;
	return sysfs_emit(buf, "%u\n", g_vib->active ? g_vib->last_duration_ms : 0);
}

static ssize_t duration_ms_store(struct kobject *kobj,
				 struct kobj_attribute *attr,
				 const char *buf, size_t count)
{
	unsigned int ms;
	int ret;

	if (!g_vib)
		return -ENODEV;

	ret = kstrtouint(buf, 0, &ms);
	if (ret)
		return ret;

	a21s_vib_trigger(g_vib, ms);
	return count;
}
static struct kobj_attribute duration_ms_attr = __ATTR_RW(duration_ms);

static struct attribute *vib_attrs[] = {
	&duration_ms_attr.attr,
	NULL,
};

static const struct attribute_group vib_attr_group = {
	.attrs = vib_attrs,
};

static int __init a21s_vib_init(void)
{
	struct a21s_vib_dev *vib;
	struct gpio_chip *gc;
	int ret;

	vib = kzalloc(sizeof(*vib), GFP_KERNEL);
	if (!vib)
		return -ENOMEM;

	mutex_init(&vib->lock);
	INIT_DELAYED_WORK(&vib->stop_work, a21s_vib_stop_work);
	INIT_WORK(&vib->ff_work, a21s_vib_ff_work);

	vib->gdev = gpio_device_find_by_label(VIB_GPIO_CHIP);
	if (!vib->gdev) {
		pr_err("a21s-vibrator: gpiochip %s not found\n", VIB_GPIO_CHIP);
		ret = -ENODEV;
		goto err_free;
	}

	gc = gpio_device_get_chip(vib->gdev);
	vib->en_desc = gpiochip_request_own_desc(gc, VIB_GPIO_LINE,
						 "a21s_motor_en",
						 GPIO_ACTIVE_HIGH,
						 GPIOD_OUT_LOW);
	if (IS_ERR(vib->en_desc)) {
		ret = PTR_ERR(vib->en_desc);
		vib->en_desc = NULL;
		pr_err("a21s-vibrator: request %s-%u failed: %d\n",
		       VIB_GPIO_CHIP, VIB_GPIO_LINE, ret);
		goto err_gdev;
	}

	vib->input = input_allocate_device();
	if (!vib->input) {
		ret = -ENOMEM;
		goto err_gpio;
	}

	vib->input->name = "a21s-vibrator";
	vib->input->id.bustype = BUS_HOST;
	input_set_drvdata(vib->input, vib);
	input_set_capability(vib->input, EV_FF, FF_RUMBLE);

	ret = input_ff_create_memless(vib->input, NULL, a21s_vib_play_effect);
	if (ret) {
		pr_err("a21s-vibrator: input_ff_create_memless failed: %d\n", ret);
		goto err_input_free;
	}

	ret = input_register_device(vib->input);
	if (ret) {
		pr_err("a21s-vibrator: input_register_device failed: %d\n", ret);
		goto err_input_free;
	}

	vib->led.name = "vibrator";
	vib->led.max_brightness = 1;
	vib->led.brightness_set_blocking = a21s_vib_led_set;
	ret = led_classdev_register(NULL, &vib->led);
	if (ret) {
		pr_warn("a21s-vibrator: led_classdev_register failed: %d\n", ret);
		vib->led.name = NULL;
	}

	g_vib = vib;
	vib->kobj = kobject_create_and_add("a21s_vib", kernel_kobj);
	if (vib->kobj) {
		ret = sysfs_create_group(vib->kobj, &vib_attr_group);
		if (ret)
			pr_warn("a21s-vibrator: sysfs_create_group failed: %d\n", ret);
	}

	pr_info("a21s-vibrator: ready (%s-%u, /sys/class/leds/vibrator, EV_FF)\n",
		VIB_GPIO_CHIP, VIB_GPIO_LINE);
	return 0;

err_input_free:
	input_free_device(vib->input);
err_gpio:
	gpiochip_free_own_desc(vib->en_desc);
err_gdev:
	gpio_device_put(vib->gdev);
err_free:
	kfree(vib);
	return ret;
}

static void __exit a21s_vib_exit(void)
{
	struct a21s_vib_dev *vib = g_vib;

	if (!vib)
		return;

	cancel_delayed_work_sync(&vib->stop_work);
	cancel_work_sync(&vib->ff_work);

	if (vib->kobj) {
		sysfs_remove_group(vib->kobj, &vib_attr_group);
		kobject_put(vib->kobj);
	}
	if (vib->led.name)
		led_classdev_unregister(&vib->led);
	if (vib->input)
		input_unregister_device(vib->input);

	a21s_vib_set_hw(vib, false);
	if (vib->en_desc)
		gpiochip_free_own_desc(vib->en_desc);
	if (vib->gdev)
		gpio_device_put(vib->gdev);

	g_vib = NULL;
	kfree(vib);
}

module_init(a21s_vib_init);
module_exit(a21s_vib_exit);

MODULE_DESCRIPTION("Samsung Galaxy A21s ERM vibration motor driver");
MODULE_LICENSE("GPL");
