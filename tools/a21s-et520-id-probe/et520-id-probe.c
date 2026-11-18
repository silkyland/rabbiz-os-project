/* SPDX-License-Identifier: GPL-2.0 */
#include <errno.h>
#include <fcntl.h>
#include <linux/spi/spidev.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <unistd.h>

#define ET5XX_READ_REGISTER 0x20
#define ET5XX_SPI_HZ 12500000

static int read_register(int fd, uint8_t address, uint8_t *value)
{
	uint8_t tx[] = { ET5XX_READ_REGISTER, address, 0 };
	uint8_t rx[sizeof(tx)] = { 0 };
	struct spi_ioc_transfer transfer = {
		.tx_buf = (uintptr_t)tx,
		.rx_buf = (uintptr_t)rx,
		.len = sizeof(tx),
		.speed_hz = ET5XX_SPI_HZ,
		.bits_per_word = 8,
	};
	int ret = ioctl(fd, SPI_IOC_MESSAGE(1), &transfer);

	if (ret != sizeof(tx)) {
		if (ret >= 0)
			errno = EIO;
		return -1;
	}

	*value = rx[2];
	return 0;
}

int main(int argc, char **argv)
{
	static const uint8_t id_registers[] = { 0xfd, 0xfe, 0xff };
	uint8_t mode, status, id[sizeof(id_registers)];
	int fd, result = 1;

	if (argc != 2) {
		fprintf(stderr, "Usage: %s /dev/spidevBUS.CS (powered ET520)\n", argv[0]);
		return 2;
	}

	fd = open(argv[1], O_RDWR | O_CLOEXEC);
	if (fd < 0) {
		perror("open SPI device");
		return 1;
	}

	if (ioctl(fd, SPI_IOC_RD_MODE, &mode) < 0) {
		perror("read SPI mode");
		goto out;
	}
	if (mode != SPI_MODE_0) {
		fprintf(stderr, "Expected SPI mode 0, got 0x%02x\n", mode);
		goto out;
	}

	if (read_register(fd, 0x00, &status) < 0) {
		perror("read ET5xx status");
		goto out;
	}
	printf("status=0x%02x\n", status);
	if (status != 0xaa)
		goto out;

	for (size_t i = 0; i < sizeof(id_registers); i++) {
		if (read_register(fd, id_registers[i], &id[i]) < 0) {
			perror("read ET5xx ID");
			goto out;
		}
	}
	printf("id=%02x:%02x:%02x\n", id[0], id[1], id[2]);
	result = id[0] == 0x03 && id[1] == 0x14 && id[2] == 0x05 ? 0 : 1;
out:
	close(fd);
	return result;
}
