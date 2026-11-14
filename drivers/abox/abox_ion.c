/* sound/soc/samsung/abox/abox_ion.c
 *
 * ALSA SoC - Samsung Abox ION buffer module
 *
 * Copyright (c) 2018 Samsung Electronics Co. Ltd.
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License version 2 as
 * published by the Free Software Foundation.
 */
#include <sound/samsung/abox.h>
#include <sound/sounddev_abox.h>

#include <linux/dma-buf.h>

#include <linux/iommu.h>
#include <linux/ion.h>

#include "abox.h"
#include "abox_ion.h"

#include <linux/dma-buf.h>
#include <linux/iosys-map.h>
#include <linux/compat.h>

/* --- mainline shim: dmabuf over coherent memory (replaces ION) --- */
struct abox_dbuf_priv {
	struct device *dev;
	void *vaddr;
	dma_addr_t dma_addr;
	size_t size;
	struct sg_table sgt;
};

static struct sg_table *abox_dbuf_map_dma_buf(struct dma_buf_attachment *attach,
		enum dma_data_direction dir)
{
	struct abox_dbuf_priv *priv = attach->dmabuf->priv;

	return &priv->sgt;
}

static void abox_dbuf_unmap_dma_buf(struct dma_buf_attachment *attach,
		struct sg_table *sgt, enum dma_data_direction dir)
{
}

static void abox_dbuf_release(struct dma_buf *dmabuf)
{
	struct abox_dbuf_priv *priv = dmabuf->priv;

	dma_free_coherent(priv->dev, priv->size, priv->vaddr, priv->dma_addr);
	sg_free_table(&priv->sgt);
	kfree(priv);
}

static int abox_dbuf_mmap(struct dma_buf *dmabuf, struct vm_area_struct *vma)
{
	struct abox_dbuf_priv *priv = dmabuf->priv;

	return dma_mmap_wc(priv->dev, vma, priv->vaddr, priv->dma_addr,
			priv->size);
}

static int abox_dbuf_vmap(struct dma_buf *dmabuf, struct iosys_map *map)
{
	struct abox_dbuf_priv *priv = dmabuf->priv;

	iosys_map_set_vaddr(map, priv->vaddr);
	return 0;
}

static void abox_dbuf_vunmap(struct dma_buf *dmabuf, struct iosys_map *map)
{
}

static const struct dma_buf_ops abox_dbuf_ops = {
	.map_dma_buf	= abox_dbuf_map_dma_buf,
	.unmap_dma_buf	= abox_dbuf_unmap_dma_buf,
	.release	= abox_dbuf_release,
	.mmap		= abox_dbuf_mmap,
	.vmap		= abox_dbuf_vmap,
	.vunmap		= abox_dbuf_vunmap,
};

static struct dma_buf *abox_ion_alloc_dmabuf(struct device *dev, size_t size)
{
	DEFINE_DMA_BUF_EXPORT_INFO(exp_info);
	struct abox_dbuf_priv *priv;
	struct dma_buf *dmabuf;
	int ret;

	priv = kzalloc(sizeof(*priv), GFP_KERNEL);
	if (!priv)
		return ERR_PTR(-ENOMEM);

	priv->dev = dev;
	priv->size = size;
	priv->vaddr = dma_alloc_coherent(dev, size, &priv->dma_addr,
			GFP_KERNEL);
	if (!priv->vaddr) {
		kfree(priv);
		return ERR_PTR(-ENOMEM);
	}

	ret = sg_alloc_table(&priv->sgt, 1, GFP_KERNEL);
	if (ret) {
		dma_free_coherent(dev, size, priv->vaddr, priv->dma_addr);
		kfree(priv);
		return ERR_PTR(ret);
	}
	sg_dma_address(priv->sgt.sgl) = priv->dma_addr;
	sg_dma_len(priv->sgt.sgl) = size;
	priv->sgt.sgl->offset = 0;
	priv->sgt.sgl->length = size;

	exp_info.exp_name = "abox-heap";
	exp_info.owner = THIS_MODULE;
	exp_info.ops = &abox_dbuf_ops;
	exp_info.size = size;
	exp_info.flags = O_RDWR | O_CLOEXEC;
	exp_info.priv = priv;

	dmabuf = dma_buf_export(&exp_info);
	if (IS_ERR(dmabuf)) {
		sg_free_table(&priv->sgt);
		dma_free_coherent(dev, size, priv->vaddr, priv->dma_addr);
		kfree(priv);
	}

	return dmabuf;
}
/* --- end shim --- */

int abox_ion_get_mmap_fd(struct device *dev,
		struct abox_ion_buf *buf,
		struct snd_pcm_mmap_fd *mmap_fd)
{
	struct dma_buf *temp_buf;

	dev_dbg(dev, "%s\n", __func__);

	if (buf->fd < 0)
		buf->fd = dma_buf_fd(buf->dma_buf, O_CLOEXEC);

	if (buf->fd < 0) {
		dev_err(dev, "%s dma_buf_fd is failed\n", __func__);
		return -EFAULT;
	}

	dev_info(dev, "%s fd(%d)\n", __func__, buf->fd);

	mmap_fd->dir = (buf->direction != DMA_FROM_DEVICE) ?
			SNDRV_PCM_STREAM_PLAYBACK : SNDRV_PCM_STREAM_CAPTURE;
	mmap_fd->size = buf->size;
	mmap_fd->actual_size = buf->size;
	mmap_fd->fd = buf->fd;

	temp_buf = dma_buf_get(buf->fd);
	if (IS_ERR(temp_buf))
		dev_err(dev, "dma_buf_get(%d) failed: %ld\n", buf->fd,
				PTR_ERR(temp_buf));

	return 0;
}

static int abox_ion_hwdep_ioctl_common(struct snd_hwdep *hw, struct file *filp,
		unsigned int cmd, void __user *arg)
{
	struct abox_ion_buf *buf = hw->private_data;
	struct device *dev = buf->dev;
	struct snd_pcm_mmap_fd mmap_fd;
	int ret;

	dev_dbg(dev, "%s(%#x)\n", __func__, cmd);

	switch (cmd) {
	case SNDRV_PCM_IOCTL_MMAP_DATA_FD:
		ret = abox_ion_get_mmap_fd(dev, buf, &mmap_fd);
		if (ret < 0) {
			dev_err(dev, "MMAP_DATA_FD failed: %d\n", ret);
			break;
		}

		if (copy_to_user(arg, &mmap_fd, sizeof(mmap_fd)))
			ret = -EFAULT;
		break;
	default:
		dev_err(dev, "unknown ioctl = %#x\n", cmd);
		ret = -ENOTTY;
		break;
	}

	return ret;
}

static int abox_ion_hwdep_ioctl(struct snd_hwdep *hw, struct file *file,
		unsigned int cmd, unsigned long arg)
{
	return abox_ion_hwdep_ioctl_common(hw, file, cmd, (void __user *)arg);
}

static int abox_ion_hwdep_ioctl_compat(struct snd_hwdep *hw, struct file *file,
		unsigned int cmd, unsigned long arg)
{
	return abox_ion_hwdep_ioctl_common(hw, file, cmd, compat_ptr(arg));
}

int abox_ion_new_hwdep(struct snd_soc_pcm_runtime *runtime,
		struct abox_ion_buf *buf, struct snd_hwdep **hwdep)
{
	struct device *dev = snd_soc_rtd_to_cpu(runtime, 0)->dev;
	char *id;
	int device = runtime->pcm->device;
	int ret;

	dev_dbg(dev, "%s\n", __func__);

	if (!buf)
		return -EINVAL;

	id = kasprintf(GFP_KERNEL, "ABOX_MMAP_FD_%d", device);
	if (!id)
		return -ENOMEM;

	ret = snd_hwdep_new(runtime->card->snd_card, id, device, hwdep);
	if (ret < 0) {
		dev_err(dev, "failed to create hwdep %s: %d\n", id, ret);
		goto out;
	}

	buf->dev = dev;
	(*hwdep)->iface = SNDRV_CTL_ELEM_IFACE_HWDEP;
	(*hwdep)->private_data = buf;
	(*hwdep)->ops.ioctl = abox_ion_hwdep_ioctl;
	(*hwdep)->ops.ioctl_compat = abox_ion_hwdep_ioctl_compat;
out:
	kfree(id);
	return ret;
}

struct abox_ion_buf *abox_ion_alloc(struct device *dev,
		struct abox_data *data,
		unsigned long iova,
		size_t size,
		bool playback)
{
	struct device *dev_abox = data->dev;
	const char *heapname = "ion_system_heap";
	struct abox_ion_buf *buf;
	int ret;

	buf = kzalloc(sizeof(*buf), GFP_KERNEL);
	if (!buf) {
		ret = -ENOMEM;
		goto error;
	}

	buf->direction = playback ? DMA_TO_DEVICE : DMA_FROM_DEVICE;
	buf->size = PAGE_ALIGN(size);
	buf->iova = iova;
	buf->fd = -EINVAL;

	buf->dma_buf = abox_ion_alloc_dmabuf(dev_abox, buf->size);
	if (IS_ERR(buf->dma_buf)) {
		ret = PTR_ERR(buf->dma_buf);
		goto error_alloc;
	}

	buf->attachment = dma_buf_attach(buf->dma_buf, dev_abox);
	if (IS_ERR(buf->attachment)) {
		ret = PTR_ERR(buf->attachment);
		goto error_attach;
	}

	buf->sgt = dma_buf_map_attachment(buf->attachment, buf->direction);
	if (IS_ERR(buf->sgt)) {
		ret = PTR_ERR(buf->sgt);
		goto error_map_dmabuf;
	}

	{
		struct iosys_map map;

		ret = dma_buf_vmap(buf->dma_buf, &map);
		if (ret) {
			ret = -ENOMEM;
			goto error_dma_buf_vmap;
		}
		buf->kva = map.vaddr;
	}

	ret = abox_iommu_map_sg(dev_abox,
			buf->iova,
			buf->sgt->sgl,
			buf->sgt->nents,
			buf->direction,
			buf->size,
			buf->kva);
	if (ret < 0) {
		dev_err(dev, "Failed to iommu_map(%pad): %d\n",
				&buf->iova, ret);
		goto error_iommu_map_sg;
	}

	return buf;

error_iommu_map_sg:
	{
		struct iosys_map map = IOSYS_MAP_INIT_VADDR(buf->kva);

		dma_buf_vunmap(buf->dma_buf, &map);
	}
error_dma_buf_vmap:
	dma_buf_unmap_attachment(buf->attachment, buf->sgt, buf->direction);
error_map_dmabuf:
	dma_buf_detach(buf->dma_buf, buf->attachment);
error_attach:
	dma_buf_put(buf->dma_buf);
error_alloc:
	kfree(buf);
error:
	dev_err(dev, "%s: Error occured while allocating\n", __func__);
	return ERR_PTR(ret);
}

int abox_ion_free(struct device *dev,
		struct abox_data *data,
		struct abox_ion_buf *buf)
{
	int ret;

	ret = abox_iommu_unmap(data->dev, buf->iova);
	if (ret < 0)
		dev_err(dev, "Failed to iommu_unmap: %d\n", ret);

	{
		struct iosys_map map = IOSYS_MAP_INIT_VADDR(buf->kva);

		dma_buf_vunmap(buf->dma_buf, &map);
	}

	if (buf->fd >= 0)
		dma_buf_put(buf->dma_buf);

	dma_buf_unmap_attachment(buf->attachment, buf->sgt, buf->direction);
	dma_buf_detach(buf->dma_buf, buf->attachment);
	dma_buf_put(buf->dma_buf);

	kfree(buf);

	return ret;
}
