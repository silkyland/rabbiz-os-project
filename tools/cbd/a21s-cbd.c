/* SPDX-License-Identifier: GPL-2.0 */
/*
 * a21s-cbd - CP firmware loader for the A21s Shannon modem.
 *
 * Skeleton replacement for the vendor `cbd` daemon's boot path:
 * streams modem.bin (or reads it directly from the RADIO GPT
 * partition) into CP RAM via /dev/umts_boot0, then runs the
 * secure-boot + bootloader sequence.
 *
 * Sequence (mirrors vendor cbd + bootdump_io_device.c):
 *   1. open("/dev/umts_boot0")
 *   2. ioctl POWER_ON
 *   3. ioctl LOAD_CP_IMAGE x N   (chunked copy into CP region)
 *   4. ioctl REQ_SECURITY        (SMC image authentication)
 *   5. ioctl START_CP_BOOTLOADER (NORMAL)
 *   6. ioctl COMPLETE_NORMAL_BOOTUP
 *   7. poll GET_CP_STATUS until ONLINE
 *
 * Build (on host with aarch64-linux-gnu-gcc or on-device):
 *   make            # produces a21s-cbd
 *   ./a21s-cbd --image modem.bin
 *   ./a21s-cbd --radio /dev/disk/by-partlabel/RADIO --offset 0 --size 0x...
 *
 * TODO:
 *  - modem.bin parsing: vendor cbd splits the image into bootloader
 *    (b_offset area) and main binary (m_offset) using the image TOC.
 *    This skeleton streams the file linearly with m_offset tracking;
 *    verify chunk semantics against vendor cbd + cp_image fields.
 *  - RADIO partition access needs the GPT layout (docs/a21s-pit.txt)
 *    or a pre-extracted modem.bin.
 *  - After ONLINE, SIPC channels come up via /dev/umts_ipc0 etc.
 *    (libsamsung-ipc / oFono take over from here).
 */
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

#include "../../drivers/cpif/cpif_uapi.h"

#define DEFAULT_DEV	"/dev/umts_boot0"
#define CHUNK_SIZE	(64 * 1024)

static void usage(const char *prog)
{
	fprintf(stderr,
		"usage: %s --image FILE | --radio DEV [--offset OFF] [--size LEN]\n"
		"       [--dev %s] [--chunk N]\n",
		prog, DEFAULT_DEV);
	exit(1);
}

static int xioctl(int fd, unsigned long req, void *arg, const char *what)
{
	int ret = ioctl(fd, req, arg);

	if (ret < 0)
		fprintf(stderr, "%s: ioctl 0x%lx failed: %s\n",
			what, req, strerror(errno));
	return ret;
}

int main(int argc, char **argv)
{
	const char *dev = DEFAULT_DEV;
	const char *image_path = NULL;
	const char *radio_dev = NULL;
	uint64_t radio_off = 0, radio_size = 0;
	size_t chunk = CHUNK_SIZE;
	int fd, i;
	struct stat st;
	void *buf;
	uint64_t total = 0;
	int ret;

	for (i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--image") && i + 1 < argc)
			image_path = argv[++i];
		else if (!strcmp(argv[i], "--radio") && i + 1 < argc)
			radio_dev = argv[++i];
		else if (!strcmp(argv[i], "--offset") && i + 1 < argc)
			radio_off = strtoull(argv[++i], NULL, 0);
		else if (!strcmp(argv[i], "--size") && i + 1 < argc)
			radio_size = strtoull(argv[++i], NULL, 0);
		else if (!strcmp(argv[i], "--dev") && i + 1 < argc)
			dev = argv[++i];
		else if (!strcmp(argv[i], "--chunk") && i + 1 < argc)
			chunk = strtoul(argv[++i], NULL, 0);
		else
			usage(argv[0]);
	}

	if (!image_path && !radio_dev)
		usage(argv[0]);

	/* ---- open source ------------------------------------------------ */
	int src;
	if (image_path) {
		src = open(image_path, O_RDONLY);
		if (src < 0) {
			perror(image_path);
			return 1;
		}
		if (fstat(src, &st)) {
			perror("fstat");
			return 1;
		}
		total = st.st_size;
	} else {
		src = open(radio_dev, O_RDONLY);
		if (src < 0) {
			perror(radio_dev);
			return 1;
		}
		if (lseek(src, (off_t)radio_off, SEEK_SET) < 0) {
			perror("lseek");
			return 1;
		}
		total = radio_size;
		if (!total) {
			/* whole partition from offset to EOF */
			off_t end = lseek(src, 0, SEEK_END);
			total = (uint64_t)(end - (off_t)radio_off);
			lseek(src, (off_t)radio_off, SEEK_SET);
		}
	}
	printf("[*] image: %llu bytes (%s)\n",
	       (unsigned long long)total,
	       image_path ? image_path : radio_dev);

	buf = malloc(chunk);
	if (!buf)
		return 1;

	/* ---- open boot device ------------------------------------------- */
	fd = open(dev, O_RDWR);
	if (fd < 0) {
		perror(dev);
		return 1;
	}

	/* 1. power on */
	ret = xioctl(fd, CPIF_IOC_POWER_ON, NULL, "POWER_ON");
	if (ret < 0)
		goto out;
	printf("[*] CP powered on\n");

	/* 2. stream image into CP region.
	 * TODO: split bootloader/main via cp_toc; for now m_offset tracks
	 * file offset linearly. Vendor cbd sends the whole image in
	 * LOAD_CP_IMAGE chunks with m_offset = target offset in CP RAM. */
	for (uint64_t off = 0; off < total; off += chunk) {
		struct cpif_image img;
		size_t n = total - off < chunk ? (size_t)(total - off) : chunk;
		ssize_t r = read(src, buf, n);
		if (r <= 0) {
			perror("read image");
			ret = -1;
			goto out;
		}
		memset(&img, 0, sizeof(img));
		img.binary = (uint64_t)(uintptr_t)buf;
		img.size = (uint32_t)total;
		img.m_offset = (uint32_t)off;   /* TODO: real mem_offset map */
		img.mode = CPIF_BOOT_MODE_NORMAL;
		img.len = (uint32_t)r;
		ret = xioctl(fd, CPIF_IOC_LOAD_CP_IMAGE, &img,
			     "LOAD_CP_IMAGE");
		if (ret < 0)
			goto out;
	}
	printf("[*] image uploaded (%llu bytes)\n",
	       (unsigned long long)total);

	/* 3. security request (SMC auth / CRC check).
	 * param2 = bootloader size, param3 = main size per
	 * shm_get_security_param2/3; TODO parse TOC for real sizes. */
	{
		struct cpif_sec_req msr;
		memset(&msr, 0, sizeof(msr));
		msr.mode = CPIF_BOOT_MODE_NORMAL;
		msr.param2 = (uint32_t)(total / 4);   /* TODO: BL size */
		msr.param3 = (uint32_t)total;          /* TODO: main size */
		ret = xioctl(fd, CPIF_IOC_REQ_SECURITY, &msr, "REQ_SECURITY");
		if (ret < 0 && ret != -EOPNOTSUPP)
			goto out;
	}

	/* 4. start CP bootloader */
	{
		struct cpif_boot_mode bm = { .idx = CPIF_BOOT_MODE_NORMAL };
		ret = xioctl(fd, CPIF_IOC_START_CP_BOOTLOADER, &bm,
			     "START_CP_BOOTLOADER");
		if (ret < 0)
			goto out;
		printf("[*] CP bootloader started\n");
	}

	/* 5. complete normal bootup */
	ret = xioctl(fd, CPIF_IOC_COMPLETE_NORMAL_BOOTUP, NULL,
		     "COMPLETE_NORMAL_BOOTUP");
	if (ret < 0 && ret != -EOPNOTSUPP)
		goto out;

	/* 6. poll status */
	for (i = 0; i < 100; i++) {
		ret = xioctl(fd, CPIF_IOC_GET_CP_STATUS, NULL, "GET_CP_STATUS");
		if (ret == CPIF_STATE_ONLINE) {
			printf("[+] modem ONLINE\n");
			break;
		}
		usleep(100 * 1000);
	}
	if (ret != CPIF_STATE_ONLINE)
		printf("[!] modem state %d after timeout\n", ret);

out:
	close(fd);
	close(src);
	free(buf);
	return ret < 0 ? 1 : 0;
}
