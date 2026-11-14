/* SPDX-License-Identifier: GPL-2.0 */
/* Compat shim: mainline removed <linux/of_gpio.h>. Provide
 * of_get_named_gpio() on top of the gpiod API so vendor drivers
 * keeping legacy int-gpio handling can compile unmodified. */
#ifndef _LINUX_OF_GPIO_COMPAT_H
#define _LINUX_OF_GPIO_COMPAT_H

#include <linux/of.h>
#include <linux/gpio.h>
#include <linux/gpio/consumer.h>
#include <linux/string.h>

static inline int of_get_named_gpio(const struct device_node *np,
				    const char *propname, int index)
{
	struct gpio_desc *desc;
	char con_id[64];
	size_t len;
	int gpio;

	if (!np)
		return -EINVAL;

	len = strnlen(propname, sizeof(con_id) - 1);
	strscpy(con_id, propname, sizeof(con_id));
	if (len >= 6 && !strcmp(con_id + len - 6, "-gpios"))
		con_id[len - 6] = '\0';
	else if (len >= 5 && !strcmp(con_id + len - 5, "-gpio"))
		con_id[len - 5] = '\0';

	desc = fwnode_gpiod_get_index(of_fwnode_handle((struct device_node *)np),
				      con_id, index, GPIOD_ASIS, "of_gpio");
	if (IS_ERR(desc))
		return PTR_ERR(desc);

	gpio = desc_to_gpio(desc);
	gpiod_put(desc);
	return gpio;
}

static inline int of_get_named_gpio_flags(const struct device_node *np,
					  const char *propname, int index,
					  unsigned long *flags)
{
	int gpio = of_get_named_gpio(np, propname, index);

	if (gpio >= 0 && flags)
		*flags = 0;
	return gpio;
}

#endif /* _LINUX_OF_GPIO_COMPAT_H */
