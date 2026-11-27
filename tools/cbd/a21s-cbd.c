/* SPDX-License-Identifier: GPL-2.0 */
/*
 * a21s-cbd - CP firmware loader for the A21s Shannon modem (s318ap).
 *
 * Parses the Samsung Shannon TOC header from --radio (/dev/disk/by-partlabel/radio)
 * or --image (modem.bin), maps each binary section (TOC, BOOT, MAIN, VSS) into
 * its CP RAM offset (mem_offset - 0x40000000) via /dev/umts_boot0, and parses
 * the in-RAM MEM\0 region map.
 */
#include <errno.h>
#include <endian.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#if defined(__has_include) && __has_include("cpif_uapi.h")
#include "cpif_uapi.h"
#else
#include "../../socs/exynos850/drivers/cpif/cpif_uapi.h"
#endif

#define DEFAULT_DEV		"/dev/umts_boot0"
#define SYSFS_CPIF_DIR		"/sys/devices/platform/soc@0/14020000.cpif"
#define CHUNK_SIZE		(64 * 1024)
#define MAX_CHUNK_SIZE		(16 * 1024 * 1024)
#define CP_MEM_BASE_ADDR	0x40000000U
#define MAX_TOC_ENTRIES		16

struct shannon_toc_entry {
	char		name[12];
	uint32_t	img_offset;
	uint32_t	mem_offset;
	uint32_t	size;
	uint32_t	crc;
	uint32_t	toc_count;
} __attribute__((packed));

static void usage(const char *prog)
{
	fprintf(stderr,
		"usage:\n"
		"  %s --load-only --radio /dev/disk/by-partlabel/radio [--dev %s]\n"
		"  %s --boot --image FILE | --radio DEV [--offset OFF] [--size LEN]\n"
		"  %s --handshake-only [--dev %s]\n"
		"  %s --status\n",
		prog, DEFAULT_DEV, prog, prog, DEFAULT_DEV, prog);
	exit(1);
}

static int boot_exchange(int fd, uint32_t request, uint32_t expected)
{
	struct timespec start, now;
	uint32_t response = 0, word = htole32(request);
	ssize_t n;

	if (write(fd, &word, sizeof(word)) != sizeof(word)) {
		perror("boot control write");
		return -1;
	}
	if (clock_gettime(CLOCK_MONOTONIC, &start))
		return -1;
	for (;;) {
		n = read(fd, &response, sizeof(response));
		if (n == sizeof(response))
			break;
		if (n < 0 && errno != EAGAIN && errno != EINTR) {
			perror("boot control read");
			return -1;
		}
		if (n > 0 || clock_gettime(CLOCK_MONOTONIC, &now))
			return -1;
		if (now.tv_sec - start.tv_sec >= 30) {
			fprintf(stderr, "boot control 0x%04x timed out\n", request);
			return -1;
		}
		usleep(10000);
	}
	response = le32toh(response);
	printf("[*] UDL 0x%04x -> 0x%04x (expected 0x%04x)\n",
	       request, response, expected);
	return response == expected ? 0 : -1;
}

static int boot_handshake(int fd)
{
	if (boot_exchange(fd, 0x900d, 0xa00d) ||
	    boot_exchange(fd, 0x9f00, 0xaf00))
		return -1;
	return 0;
}

static int xioctl(int fd, unsigned long req, void *arg, const char *what)
{
	int ret = ioctl(fd, req, arg);

	if (ret < 0)
		fprintf(stderr, "%s: ioctl 0x%lx failed: %s\n",
			what, req, strerror(errno));
	return ret;
}

static void print_sysfs_attr(const char *attr)
{
	char path[256];
	char buf[512];
	int fd;
	ssize_t n;

	snprintf(path, sizeof(path), "%s/%s", SYSFS_CPIF_DIR, attr);
	fd = open(path, O_RDONLY);
	if (fd < 0) {
		printf("%s: (unavailable)\n", attr);
		return;
	}
	n = read(fd, buf, sizeof(buf) - 1);
	close(fd);
	if (n > 0) {
		buf[n] = '\0';
		while (n > 0 && (buf[n - 1] == '\n' || buf[n - 1] == '\r'))
			buf[--n] = '\0';
		printf("%s: %s\n", attr, buf);
	}
}

static int show_status(void)
{
	print_sysfs_attr("modem_name");
	print_sysfs_attr("phone_state");
	print_sysfs_attr("rmem_info");
	print_sysfs_attr("toc_summary");
	print_sysfs_attr("mem_map_on_cp");
	print_sysfs_attr("regions_summary");
	return 0;
}

static int upload_range(int boot_fd, int src_fd, uint64_t file_off,
			uint32_t cp_off, uint32_t size, uint32_t total_size,
			void *buf, size_t chunk, uint32_t mode)
{
	uint32_t done = 0;

	if (lseek(src_fd, (off_t)file_off, SEEK_SET) < 0) {
		perror("lseek src");
		return -1;
	}

	while (done < size) {
		struct cpif_image img;
		size_t want = (size - done) < chunk ? (size_t)(size - done) : chunk;
		ssize_t got = read(src_fd, buf, want);

		if (got <= 0) {
			perror("read firmware chunk");
			return -1;
		}
		memset(&img, 0, sizeof(img));
		img.binary = (uint64_t)(uintptr_t)buf;
		img.size = total_size;
		img.m_offset = cp_off + done;
		img.mode = mode;
		img.len = (uint32_t)got;

		if (xioctl(boot_fd, CPIF_IOC_LOAD_CP_IMAGE, &img,
			   "LOAD_CP_IMAGE") < 0)
			return -1;
		done += (uint32_t)got;
	}
	return 0;
}

int main(int argc, char **argv)
{
	const char *dev = DEFAULT_DEV;
	const char *image_path = NULL;
	const char *radio_dev = NULL;
	uint64_t radio_off = 0, radio_size = 0;
	size_t chunk = CHUNK_SIZE;
	int load_only = 1;
	int status_only = 0;
	int handshake_only = 0;
	int fd = -1, src = -1, i;
	struct stat st;
	void *buf = NULL;
	uint64_t total = 0;
	uint32_t boot_sz = 0, main_sz = 0;
	struct shannon_toc_entry toc[MAX_TOC_ENTRIES];
	int is_toc = 0;
	int ret = 0;

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
		else if (!strcmp(argv[i], "--chunk") && i + 1 < argc) {
			char *endp = NULL;
			unsigned long val;
			errno = 0;
			val = strtoul(argv[++i], &endp, 0);
			if (errno != 0 || !endp || *endp != '\0' ||
			    val == 0 || val > MAX_CHUNK_SIZE) {
				fprintf(stderr,
					"invalid --chunk size (must be 1..%u)\n",
					(unsigned)MAX_CHUNK_SIZE);
				return 1;
			}
			chunk = (size_t)val;
		}
		else if (!strcmp(argv[i], "--load-only"))
			load_only = 1;
		else if (!strcmp(argv[i], "--boot"))
			load_only = 0;
		else if (!strcmp(argv[i], "--status"))
			status_only = 1;
		else if (!strcmp(argv[i], "--handshake-only"))
			handshake_only = 1;
		else
			usage(argv[0]);
	}

	if (status_only)
		return show_status();

	if (handshake_only) {
		fd = open(dev, O_RDWR | O_NONBLOCK);
		if (fd < 0) {
			perror(dev);
			return 1;
		}
		ret = boot_handshake(fd);
		close(fd);
		return ret ? 1 : 0;
	}

	if (!image_path && !radio_dev)
		usage(argv[0]);

	src = open(image_path ? image_path : radio_dev, O_RDONLY);
	if (src < 0) {
		perror(image_path ? image_path : radio_dev);
		return 1;
	}

	if (image_path && !fstat(src, &st) && st.st_size > 0) {
		total = (uint64_t)st.st_size;
	} else {
		off_t end = lseek(src, 0, SEEK_END);
		if (end < 0) {
			perror("lseek SEEK_END");
			close(src);
			return 1;
		}
		if (!radio_size) {
			if (radio_off > (uint64_t)end) {
				fprintf(stderr, "offset exceeds input size\n");
				close(src);
				return 1;
			}
			total = (uint64_t)end - radio_off;
		} else {
			total = radio_size;
		}
	}

	if (lseek(src, (off_t)radio_off, SEEK_SET) < 0 ||
	    read(src, toc, sizeof(toc)) != sizeof(toc)) {
		perror("read TOC");
		close(src);
		return 1;
	}
	if (!strncmp(toc[0].name, "TOC", 3))
		is_toc = 1;

	buf = malloc(chunk);
	if (!buf) {
		close(src);
		return 1;
	}

	fd = open(dev, O_RDWR | O_NONBLOCK);
	if (fd < 0) {
		perror(dev);
		ret = -1;
		goto out;
	}

	if (!load_only) {
		ret = xioctl(fd, CPIF_IOC_POWER_ON, NULL, "POWER_ON");
		if (ret < 0)
			goto out;
		printf("[*] CP powered on\n");
	} else {
		printf("[*] mode: --load-only (safe cp_rmem upload without PMU/SMC reset)\n");
	}

	if (is_toc) {
		uint32_t count = toc[0].toc_count;
		uint64_t loaded = 0;

		if (count == 0 || count > MAX_TOC_ENTRIES)
			count = MAX_TOC_ENTRIES;

		for (uint32_t k = 0; k < count; k++) {
			char name[13] = { 0 };
			uint32_t cp_off;

			if (!toc[k].name[0])
				break;
			memcpy(name, toc[k].name, 12);

			/* Skip NV when it has no payload in radio (EFS-backed) */
			if (!strcmp(name, "NV") && toc[k].img_offset == 0) {
				printf("[*] skip toc[%u] %-6s (EFS NV placeholder, size 0x%x)\n",
				       k, name, toc[k].size);
				continue;
			}
			if (toc[k].size == 0)
				continue;

			cp_off = toc[k].mem_offset >= CP_MEM_BASE_ADDR ?
				(toc[k].mem_offset - CP_MEM_BASE_ADDR) :
				toc[k].mem_offset;

			printf("[*] load toc[%u] %-6s file 0x%08x -> cp_rmem+0x%08x (%u bytes)\n",
			       k, name, toc[k].img_offset, cp_off, toc[k].size);

			ret = upload_range(fd, src, radio_off + toc[k].img_offset,
					   cp_off, toc[k].size, toc[k].size,
					   buf, chunk, CPIF_BOOT_MODE_NORMAL);
			if (ret < 0)
				goto out;

			if (!strcmp(name, "BOOT"))
				boot_sz = toc[k].size;
			else if (!strcmp(name, "MAIN"))
				main_sz = toc[k].size;
			loaded += toc[k].size;
		}
		printf("[*] TOC sections uploaded (%llu bytes total)\n",
		       (unsigned long long)loaded);
	} else {
		ret = upload_range(fd, src, radio_off, 0, (uint32_t)total,
				   (uint32_t)total, buf, chunk,
				   CPIF_BOOT_MODE_NORMAL);
		if (ret < 0)
			goto out;
		boot_sz = (uint32_t)(total / 4);
		main_sz = (uint32_t)total;
		printf("[*] linear image uploaded (%llu bytes)\n",
		       (unsigned long long)total);
	}

	if (load_only) {
		uint32_t st_online = CPIF_STATE_ONLINE;

		ret = xioctl(fd, CPIF_IOC_SET_CP_STATUS, &st_online,
			     "SET_CP_STATUS");
		if (ret < 0)
			goto out;
		printf("[+] CP firmware mapped in cp_rmem (TOC validated)\n");
		show_status();
		goto out;
	}

	{
		struct cpif_sec_req msr;
		memset(&msr, 0, sizeof(msr));
		msr.mode = CPIF_BOOT_MODE_NORMAL;
		msr.param2 = boot_sz;
		msr.param3 = main_sz;
		ret = xioctl(fd, CPIF_IOC_REQ_SECURITY, &msr, "REQ_SECURITY");
		if (ret < 0 && ret != -EOPNOTSUPP)
			goto out;
	}

	{
		struct cpif_boot_mode bm = { .idx = CPIF_BOOT_MODE_NORMAL };
		ret = xioctl(fd, CPIF_IOC_START_CP_BOOTLOADER, &bm,
			     "START_CP_BOOTLOADER");
		if (ret < 0)
			goto out;
		printf("[*] CP bootloader started\n");
	}

	ret = boot_handshake(fd);
	if (ret)
		goto out;

	ret = xioctl(fd, CPIF_IOC_COMPLETE_NORMAL_BOOTUP, NULL,
		     "COMPLETE_NORMAL_BOOTUP");
	if (ret < 0 && ret != -EOPNOTSUPP)
		goto out;

	for (i = 0; i < 100; i++) {
		ret = xioctl(fd, CPIF_IOC_GET_CP_STATUS, NULL, "GET_CP_STATUS");
		if (ret == CPIF_STATE_ONLINE) {
			printf("[+] modem ONLINE\n");
			break;
		}
		usleep(100 * 1000);
	}

out:
	if (fd >= 0)
		close(fd);
	if (src >= 0)
		close(src);
	free(buf);
	return ret < 0 ? 1 : 0;
}
