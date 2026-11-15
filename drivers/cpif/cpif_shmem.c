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
	u32	name;		/* "CPI", "SSV", "GOL", "B2L", "PKP", "APV" */
	u32	offset;
	u32	size;
} __packed;

struct cpif_mem_map {
	__be32	version;	/* "MEM\0" */
	u32	secure_size;
	u32	ns_size;
	u32	ns_map_count;
	struct cpif_ns_map_info ns_map[CPIF_MAX_MAP_ON_CP];
	u32	mem_guard;
} __packed;

struct cpif_toc {
	char	name[12];
	u32	img_offset;
	u32	mem_offset;
	u32	size;
	u32	crc;
	u32	reserved;
} __packed;

/* ---- region table ------------------------------------------------- */

static const char * const cpif_shmem_names[CPIF_SHMEM_MAX] = {
	[CPIF_SHMEM_CP]		= "CP",
	[CPIF_SHMEM_VSS]	= "VSS",
	[CPIF_SHMEM_L2B]	= "L2B",
	[CPIF_SHMEM_IPC]	= "IPC",
	[CPIF_SHMEM_VPA]	= "VPA",
	[CPIF_SHMEM_BTL]	= "BTL", /* vendor also uses "GOL" */
	[CPIF_SHMEM_PKTPROC]	= "PKP",
};

/* Vendor "region,name" strings -> our indices */
static int cpif_region_index(const char *name)
{
	int i;

	if (!strcmp(name, "GOL"))
		return CPIF_SHMEM_BTL;

	for (i = 0; i < CPIF_SHMEM_MAX; i++) {
		if (cpif_shmem_names[i] && !strcmp(name, cpif_shmem_names[i]))
			return i;
	}
	return -1;
}

static int cpif_shmem_parse_dt(struct cpif_dev *c)
{
	struct device_node *np = c->dev->of_node;
	struct device_node *shmem_np, *regions, *child;
	int count = 0;

	shmem_np = of_get_child_by_name(np, "cp-shmem");
	if (!shmem_np)
		shmem_np = of_get_child_by_name(np, "cp_shmem");
	if (!shmem_np)
		shmem_np = of_find_node_by_name(NULL, "cp-shmem");
	if (!shmem_np)
		shmem_np = of_find_node_by_name(NULL, "cp_shmem");
	if (!shmem_np) {
		dev_warn(c->dev, "no cp-shmem/cp_shmem node, regions unmapped\n");
		return 0;
	}

	regions = of_get_child_by_name(shmem_np, "regions");
	if (!regions)
		regions = of_node_get(shmem_np);

	for_each_child_of_node(regions, child) {
		struct cpif_shmem_region *r;
		const char *region_name = NULL;
		u32 off, size, cached = 0;
		u32 idx;
		int i;

		if (of_property_read_string(child, "region,name",
					    &region_name))
			continue;
		if (of_property_read_u32(child, "region,index", &idx))
			continue;
		if (of_property_read_u32(child, "region,offset", &off))
			continue;
		if (of_property_read_u32(child, "region,size", &size))
			continue;

		i = cpif_region_index(region_name);
		if (i < 0 || i >= CPIF_SHMEM_MAX)
			continue;

		r = &c->shmem[i];
		r->name = kstrdup(region_name, GFP_KERNEL);
		r->phys = c->cp_base + (phys_addr_t)off;
		r->size = size;
		if (!of_property_read_u32(child, "region,cached", &cached))
			r->cached = !!cached;
		else
			r->cached = false;

		/* Map every region as write-combining normal memory so
		 * ARM64 unaligned copies and memcpy_toio do not fault. */
		r->map = ioremap_wc(r->phys, r->size);
		if (!r->map)
			r->map = ioremap(r->phys, r->size);
		if (!r->map) {
			dev_err(c->dev, "ioremap %s@0x%llx failed\n",
				region_name, (u64)r->phys);
			continue;
		}

		dev_info(c->dev, "shmem %-3s phys 0x%08llx size 0x%zx%s\n",
			 r->name ?: region_name, (u64)r->phys, r->size,
			 r->cached ? " (cached)" : "");
		count++;
	}

	of_node_put(regions);
	of_node_put(shmem_np);

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
		kfree(c->shmem[i].name);
		c->shmem[i].name = NULL;
	}
}

void __iomem *cpif_shmem_map(struct cpif_dev *c, int idx)
{
	if (idx < 0 || idx >= CPIF_SHMEM_MAX)
		return NULL;
	return c->shmem[idx].map;
}

void __iomem *cpif_shmem_resolve(struct cpif_dev *c, u32 m_offset, u32 len)
{
	phys_addr_t phys = c->cp_base + (phys_addr_t)m_offset;
	int i;

	for (i = 0; i < CPIF_SHMEM_MAX; i++) {
		struct cpif_shmem_region *r = &c->shmem[i];
		phys_addr_t end;

		if (!r->map || !r->size)
			continue;
		end = r->phys + r->size;
		if (phys >= r->phys && len <= r->size &&
		    phys - r->phys <= r->size - len)
			return r->map + (phys - r->phys);
		(void)end;
	}
	return NULL;
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
 * (either at CP+0x0 in linear layout or CP+0x8000 in Shannon split RAM
 * layout) and inspects struct cp_mem_map at BOOT + 0xA0.
 */
int cpif_dump_toc(struct cpif_dev *c)
{
	void __iomem *base = cpif_shmem_map(c, CPIF_SHMEM_CP);
	size_t cp_sz = cpif_shmem_size(c, CPIF_SHMEM_CP);
	struct cpif_toc toc;
	struct cpif_mem_map map;
	u32 toc_off = 0, boot_off = 0;
	u32 max_entries = 16;
	int pos = 0;
	int i;

	if (!base) {
		dev_warn(c->dev, "dump_toc: CP region not mapped\n");
		return -ENODEV;
	}

	c->toc_sections = 0;
	c->toc_summary[0] = '\0';
	c->mem_map_on_cp = false;

	/* Detect split Shannon layout (TOC @ +0x8000, BOOT @ +0x0) vs
	 * raw linear radio image (TOC @ +0x0, BOOT follows TOC). */
	if (cp_sz > 0x8000 + sizeof(toc)) {
		memcpy_fromio(&toc, base + 0x8000, sizeof(toc));
		if (!strncmp(toc.name, "TOC", 3))
			toc_off = 0x8000;
	}
	if (!toc_off) {
		memcpy_fromio(&toc, base, sizeof(toc));
		if (strncmp(toc.name, "TOC", 3) != 0) {
			dev_info(c->dev, "CP image TOC not loaded yet\n");
			return 0;
		}
	}

	if (toc.reserved > 0 && toc.reserved < 16)
		max_entries = toc.reserved;

	dev_info(c->dev, "CP image TOC @ phys 0x%llx (off 0x%x, entries %u):\n",
		 (u64)cpif_shmem_base(c, CPIF_SHMEM_CP) + toc_off,
		 toc_off, max_entries);

	for (i = 0; i < (int)max_entries; i++) {
		char name[13] = { 0 };

		memcpy_fromio(&toc, base + toc_off + i * sizeof(toc),
			      sizeof(toc));
		if (!toc.name[0])
			break;
		memcpy(name, toc.name, 12);
		dev_info(c->dev,
			 "  toc[%d] %-12s img_off 0x%08x mem_off 0x%08x "
			 "size 0x%08x crc 0x%08x\n",
			 i, name, toc.img_offset, toc.mem_offset,
			 toc.size, toc.crc);

		if (pos < (int)sizeof(c->toc_summary) - 20)
			pos += scnprintf(c->toc_summary + pos,
					 sizeof(c->toc_summary) - pos,
					 "%s%s", pos ? "," : "", name);
		c->toc_sections++;

		if (!strcmp(name, "BOOT") && toc_off == 0 &&
		    toc.img_offset < cp_sz - CPIF_MAP_ON_CP_OFFSET)
			boot_off = toc.img_offset;
	}

	if (cp_sz > boot_off + CPIF_MAP_ON_CP_OFFSET + sizeof(map)) {
		u32 ver;

		memcpy_fromio(&map, base + boot_off + CPIF_MAP_ON_CP_OFFSET,
			      sizeof(map));
		ver = be32_to_cpu(map.version);
		if (!memcmp(&ver, "MEM", 3)) {
			c->mem_map_on_cp = true;
			dev_info(c->dev,
				 "mem_map_on_cp: secure 0x%x ns 0x%x count %u\n",
				 map.secure_size, map.ns_size,
				 map.ns_map_count);
			for (i = 0; i < (int)map.ns_map_count &&
			     i < CPIF_MAX_MAP_ON_CP; i++) {
				char rname[5] = { 0 };

				memcpy(rname, &map.ns_map[i].name, 4);
				dev_info(c->dev,
					 "  ns_map[%d] %-4s off 0x%08x size 0x%08x\n",
					 i, rname, map.ns_map[i].offset,
					 map.ns_map[i].size);
			}
			return 0;
		}
	}

	dev_info(c->dev, "mem_map_on_cp: not found (DT layout assumed)\n");
	return 0;
}
