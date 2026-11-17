// SPDX-License-Identifier: GPL-2.0
/*
 * A21s SBD v1 bootstrap, following vendor create_sbd_mem_map/init_sbd_link.
 * This publishes real buffer descriptors, not a working userspace data plane.
 */
#include <linux/io.h>
#include <linux/slab.h>
#include <linux/unaligned.h>

#include "cpif.h"

#define CPIF_SBD_DESC_OFF	0x800
#define CPIF_SBD_DATA_OFF	0x10000
#define CPIF_SBD_MAX_CHANNELS	32

struct cpif_sbd_rb_desc {
	__le16 ch;
	__le16 reserved;
	__le16 direction;
	__le16 signaling;
	__le32 sig_mask;
	__le16 length;
	__le16 id;
	__le16 buff_size;
	__le16 payload_offset;
} __packed;

struct cpif_sbd_rb_channel {
	__le32 rb_desc_offset;
	__le32 buff_pos_array_offset;
} __packed;

struct cpif_sbd_global_desc {
	__le32 version;
	__le32 num_channels;
	__le32 rpwp_array_offset;
	struct cpif_sbd_rb_channel channel[CPIF_SBD_MAX_CHANNELS][2];
	struct cpif_sbd_rb_desc rb[CPIF_SBD_MAX_CHANNELS][2];
} __packed;

static_assert(sizeof(struct cpif_sbd_rb_desc) == 20);
static_assert(sizeof(struct cpif_sbd_global_desc) == 1804);

/* Vendor a21s_eur_open_w00_r06 DT, in init_ctrl_tables() order.
 * PDP netdevs share the last two QoS rings; regional SKT/SPR rings are excluded.
 */
static const struct {
	u16 channel;
	u16 length[2];
	u16 size[2];
} cpif_sbd_channels[] = {
	{ 35,  { 16, 16 },    { 2048, 2048 } },
	{ 235, { 16, 32 },    { 4096, 4096 } },
	{ 236, { 16, 32 },    { 4096, 4096 } },
	{ 245, { 16, 512 },   { 2048, 2048 } },
	{ 1,   { 32, 64 },    { 2048, 2048 } },
	{ 25,  { 16, 16 },    { 2048, 2048 } },
	{ 28,  { 16, 128 },   { 2048, 2048 } },
	{ 244, { 16, 32 },    { 2048, 2048 } },
	{ 10,  { 256, 1024 }, { 2048, 2048 } },
	{ 11,  { 512, 1024 }, { 2048, 2048 } },
};

int cpif_sbd_bootstrap(struct cpif_dev *c)
{
	struct cpif_shmem_region *r = &c->shmem[CPIF_SHMEM_IPC];
	struct cpif_sbd_global_desc *g;
	u32 desc, data = CPIF_SBD_DATA_OFF;
	u8 *image;
	int id, dir, ret = 0;

	lockdep_assert_held(&c->lock);
	if (c->sbd_ready)
		return 0;
	if (!r->map || r->size < CPIF_SBD_DATA_OFF)
		return -ENODEV;
	image = kzalloc(CPIF_SBD_DATA_OFF, GFP_KERNEL);
	if (!image)
		return -ENOMEM;
	g = (void *)(image + CPIF_SBD_DESC_OFF);
	g->version = cpu_to_le32(1);
	g->num_channels = cpu_to_le32(ARRAY_SIZE(cpif_sbd_channels));
	desc = CPIF_SBD_DESC_OFF + sizeof(*g);
	g->rpwp_array_offset = cpu_to_le32(desc);
	/* Four u16 arrays: UL read/write, DL read/write. Initially all zero. */
	desc += 4 * sizeof(u16) * ARRAY_SIZE(cpif_sbd_channels);

	for (id = 0; id < ARRAY_SIZE(cpif_sbd_channels); id++) {
		for (dir = 0; dir < 2; dir++) {
			struct cpif_sbd_rb_desc *rb = &g->rb[id][dir];
			u32 len = cpif_sbd_channels[id].length[dir];
			u32 size = cpif_sbd_channels[id].size[dir];
			u32 slot;

			if (desc + 2 * len * sizeof(u32) > CPIF_SBD_DATA_OFF ||
			    data + len * size > r->size) {
				ret = -ENOSPC;
				goto out;
			}
			g->channel[id][dir].rb_desc_offset =
				cpu_to_le32((u8 *)rb - image);
			g->channel[id][dir].buff_pos_array_offset = cpu_to_le32(desc);
			rb->ch = cpu_to_le16(cpif_sbd_channels[id].channel);
			rb->direction = cpu_to_le16(dir);
			rb->signaling = cpu_to_le16(1);
			rb->sig_mask = cpu_to_le32(0x81);
			rb->length = cpu_to_le16(len);
			rb->id = cpu_to_le16(id);
			rb->buff_size = cpu_to_le16(size);
			for (slot = 0; slot < len; slot++)
				put_unaligned_le32(data + slot * size,
						   image + desc + slot * sizeof(u32));
			/* Buffer lengths follow buffer positions and remain zero. */
			desc += 2 * len * sizeof(u32);
			data += len * size;
		}
	}
	memcpy_toio(r->map + CPIF_SBD_DESC_OFF, image + CPIF_SBD_DESC_OFF,
		    CPIF_SBD_DATA_OFF - CPIF_SBD_DESC_OFF);
	/* Publish every descriptor and buffer position before PIF_INIT_DONE. */
	wmb();
	if (readl(r->map + CPIF_SBD_DESC_OFF) != 1 ||
	    readl(r->map + CPIF_SBD_DESC_OFF + 4) != ARRAY_SIZE(cpif_sbd_channels)) {
		ret = -EIO;
		goto out;
	}
	c->sbd_ready = true;
	dev_info(c->dev, "SBD v1: %zu channels, descriptors end 0x%x, buffers end 0x%x\n",
		 ARRAY_SIZE(cpif_sbd_channels), desc, data);
out:
	kfree(image);
	return ret;
}
