// SPDX-License-Identifier: GPL-2.0
/*
 * CP shared-memory handling for the A21s cpif skeleton.
 *
 * Two parts, both transcribed from vendor shm_ipc.c:
 *
 *  1. DT-declared regions: cp_shmem/regions children give offset+size
 *     inside the contiguous cp_rmem block (0xd0000000, 136 MiB).
 *     Names match vendor: CP, VSS, IPC, BTL/L2B, PKP ...
 *
 *  2. TOC/mem-map: after firmware upload, the CP image starts with a
 *     table of struct cp_toc entries. Each entry has img_offset which
 *     points (via MAP_ON_CP_OFFSET=0xA0) to a struct cp_mem_map
 *     describing secure/NS regions carved out of cp_rmem. This lets
 *     userspace locate IPC queues without hardcoding offsets.
 */
#include <linux/io.h>
#include <linux/of.h>
#include <linux/of_address.h>
#include <linux/slab.h>
#include <linux/string.h>

#include "cpif.h"

/* ---- structures identical to vendor shm_ipc.c ---- */

#define CPIF_MAX_MAP_ON_CP	10
#define CPIF_MAP_ON_CP_OFFSET	0xA0

struct cpif_ns_map_info {
	__be32	name;		/* "CPI", "SSV", "GOL", "B2L", "PKP", "APV" */
	__be32	offset;
	__be32	size;
	__be32	reserved;
} __packed;

struct cpif_mem_map {
	__be32	version;	/* "MEM\0" */
	__be32	secure_size;
	__be32	ns_size;
	__be32	ns_map_count;
	struct cpif_ns_map_info ns_map[CPIF_MAX_MAP_ON_CP];
	__be32	mem_guard;
} __packed;

struct cpif_toc {
	char	name[12];
	__be32	img_offset;
	__be32	mem_offset;
	__be32	size;
	__be32	crc;
	__be32	reserved;
} __packed;

/* ---- region table ------------------------------------------------- */

static const char * const cpif_shmem_names[CPIF_SHMEM_MAX] = {
	[CPIF_SHMEM_CP]		= "CP",
	[CPIF_SHMEM_VSS]	= "VSS",
	[CPIF_SHMEM_L2B]	= "L2B",
	[CPIF_SHMEM_IPC]	= "IPC",
	[CPIF_SHMEM_VPA]	= "VPA",
	[CPIF_SHMEM_BTL]	= "GOL", /* vendor uses "GOL" for BTL index */
	[CPIF_SHMEM_PKTPROC]	= "PKP",
};

/* Vendor "region,name" strings -> our indices */
static int cpif_region_index(const char *name)
{
	int i;

	for (i = 0; i < CPIF_SHMEM_MAX; i++) {
		if (cpif_shmem_names[i] && !strcmp(name, cpif_shmem_names[i]))
			return i;
	}
	return -1;
}

static int cpif_shmem_parse_dt(struct cpif_dev *c)
{
	struct device_node *np = c->dev->of_node;
	struct device_node *regions, *child;
	int count = 0;

	regions = of_get_child_by_name(np, "cp_shmem");
	if (!regions) {
		regions = of_find_node_by_name(NULL, "cp_shmem");
		if (!regions) {
			dev_warn(c->dev, "no cp_shmem node, regions unmapped\n");
			return 0;
		}
	}

	for_each_child_of_node(regions, child) {
		struct cpif_shmem_region *r;
		const char *region_name = NULL;
		__be32 off, size, cached;
		u32 idx;
		int i;

		if (of_property_read_string(child, "region,name",
					    &region_name))
			continue;
		if (of_property_read_u32(child, "region,index", &idx))
			continue;
		if (of_property_read_u32(child, "region,offset",
					   &off))
			continue;
		if (of_property_read_u32(child, "region,size",
					   &size))
			continue;

		i = cpif_region_index(region_name);
		if (i < 0 || i >= CPIF_SHMEM_MAX)
			continue;

		r = &c->shmem[i];
		r->name = region_name;
		r->phys = c->cp_base + (phys_addr_t)be32_to_cpu(off);
		r->size = be32_to_cpu(size);
		if (!of_property_read_u32(child, "region,cached", &cached))
			r->cached = !!be32_to_cpu(cached);
		else
			r->cached = false;

		/* Map every region: 136 MiB fits fine and keeps
		 * TOC parsing trivial. TODO: map lazily if needed. */
		r->map = ioremap(r->phys, r->size);
		if (!r->map) {
			dev_err(c->dev, "ioremap %s@0x%llx failed\n",
				region_name, (u64)r->phys);
			continue;
		}

		dev_info(c->dev, "shmem %-3s phys 0x%08llx size 0x%zx%s\n",
			 region_name, (u64)r->phys, r->size,
			 r->cached ? " (cached)" : "");
		count++;
	}

	dev_info(c->dev, "cp_shmem: %d regions mapped\n", count);
	return 0;
}

int cpif_shmem_init(struct cpif_dev *c)
{
	return cpif_shmem_parse_dt(c);
}

void cpif_shmem_exit(struct cpif_dev *c)
{
	int i;

	for (i = 0; i < CPIF_SHMEM_MAX; i++) {
		if (c->shmem[i].map) {
			iounmap(c->shmem[i].map);
			c->shmem[i].map = NULL;
		}
	}
}

void __iomem *cpif_shmem_map(struct cpif_dev *c, int idx)
{
	if (idx < 0 || idx >= CPIF_SHMEM_MAX)
		return NULL;
	return c->shmem[idx].map;
}

phys_addr_t cpif_shmem_base(struct cpif_dev *c, int idx)
{
	if (idx < 0 || idx >= CPIF_SHMEM_MAX)
		return 0;
	return c->shmem[idx].phys;
}

size_t cpif_shmem_size(struct cpif_dev *c, int idx)
{
	if (idx < 0 || idx >= CPIF_SHMEM_MAX)
		return 0;
	return c->shmem[idx].size;
}

/* ---- TOC dump (debug helper for cbd bring-up) ----------------------
 *
 * Vendor: cp_shmem_check_mem_map_on_cp() reads struct cp_toc entries
 * starting at cp_shmem[CP]+sizeof(cp_toc) — the firmware image has a
 * TOC header followed by entries; toc->img_offset then points into
 * the image where +0xA0 sits struct cp_mem_map.
 */
int cpif_dump_toc(struct cpif_dev *c)
{
	void __iomem *base = cpif_shmem_map(c, CPIF_SHMEM_CP);
	struct cpif_toc toc;
	int i;

	if (!base) {
		dev_warn(c->dev, "dump_toc: CP region not mapped\n");
		return -ENODEV;
	}

	dev_info(c->dev, "CP image TOC @ phys 0x%llx:\n",
		 (u64)cpif_shmem_base(c, CPIF_SHMEM_CP));

	/* entries start after the image header toc; cap at 16 entries */
	for (i = 0; i < 16; i++) {
		memcpy_fromio(&toc, base + sizeof(toc) + i * sizeof(toc),
			      sizeof(toc));
		if (!toc.name[0])
			break;
		dev_info(c->dev,
			 "  toc[%d] %-12s img_off 0x%08x mem_off 0x%08x "
			 "size 0x%08x crc 0x%08x\n",
			 i, toc.name, be32_to_cpu(toc.img_offset),
			 be32_to_cpu(toc.mem_offset), be32_to_cpu(toc.size),
			 be32_to_cpu(toc.crc));
	}

	/* try to find the MEM map: scan toc entries for a plausible
	 * img_offset pointing at a "MEM\0" version field */
	for (i = 0; i < 16; i++) {
		struct cpif_mem_map map;
		phys_addr_t off;

		memcpy_fromio(&toc, base + sizeof(toc) + i * sizeof(toc),
			      sizeof(toc));
		if (!toc.name[0])
			break;
		off = (phys_addr_t)be32_to_cpu(toc.img_offset) +
		      CPIF_MAP_ON_CP_OFFSET;
		if (off >= cpif_shmem_size(c, CPIF_SHMEM_CP))
			continue;
		memcpy_fromio(&map, base + off, sizeof(map));
		if (map.version == 0) {
			char v[5] = { 0 };
			memcpy(v, &map.version, 4);
			if (!memcmp(v, "MEM", 3)) {
				c->mem_map_on_cp = true;
				dev_info(c->dev,
					 "mem_map_on_cp: secure 0x%x ns 0x%x "
					 "count %u\n",
					 be32_to_cpu(map.secure_size),
					 be32_to_cpu(map.ns_size),
					 be32_to_cpu(map.ns_map_count));
				return 0;
			}
		}
	}

	dev_info(c->dev, "mem_map_on_cp: not found (DT layout assumed)\n");
	return 0;
}
